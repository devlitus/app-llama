//! Reintentos de una transferencia ante cortes, con espera inyectada.

use std::path::Path;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::header::HeaderValue;
use tokio_util::sync::CancellationToken;

use super::error::DownloadError;
use super::model::{FailureKind, FileSpec};
use super::paths::part_path;
use super::transfer::{HttpFetcher, TransferOutcome, part_len, reset_part, transfer_once};

/// Espera inyectada: los tests no duermen de verdad.
#[async_trait]
pub trait Sleeper: Send + Sync {
    async fn sleep(&self, duration: Duration);
}

pub struct TokioSleeper;

#[async_trait]
impl Sleeper for TokioSleeper {
    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts_without_progress: u32,
    pub backoff: [Duration; 4],
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts_without_progress: 5,
            backoff: [1, 2, 4, 8].map(Duration::from_secs),
        }
    }
}

/// Seguimiento de los intentos de una llamada a `transfer_with_retry`.
struct Attempts {
    without_progress: u32,
    peak_len: u64,
    restarted_after_bad_range: bool,
}

impl Attempts {
    /// Solo cuenta como progreso superar la marca máxima: tras un reinicio,
    /// volver a descargar lo que ya se tenía no lo es.
    fn record(&mut self, part_len: u64) {
        if part_len > self.peak_len {
            self.peak_len = part_len;
            self.without_progress = 0;
        } else {
            self.without_progress += 1;
        }
    }
}

impl RetryPolicy {
    fn is_exhausted(&self, attempts: &Attempts) -> bool {
        attempts.without_progress >= self.max_attempts_without_progress
    }

    fn delay_after(&self, attempts: &Attempts) -> Option<Duration> {
        let index = usize::try_from(attempts.without_progress.checked_sub(1)?).ok()?;
        Some(self.backoff[index.min(self.backoff.len() - 1)])
    }
}

/// Avisos de una transferencia hacia quien la lanza.
pub struct TransferObserver<'a> {
    /// Bytes nuevos escritos en el `.part`: solo deltas.
    pub on_bytes: &'a mut (dyn FnMut(u64) + Send),
    /// Una sola vez por reinicio, ya con el `.part` truncado y antes de la espera.
    pub on_restart: &'a mut (dyn FnMut() + Send),
}

/// `auth` se evalúa antes de cada intento: el token puede cambiar entre dos.
pub async fn transfer_with_retry(
    fetcher: &HttpFetcher,
    spec: &FileSpec,
    auth: &(dyn Fn() -> Option<HeaderValue> + Send + Sync),
    policy: &RetryPolicy,
    sleeper: &dyn Sleeper,
    cancel: &CancellationToken,
    observer: TransferObserver<'_>,
) -> Result<TransferOutcome, DownloadError> {
    let TransferObserver {
        on_bytes,
        on_restart,
    } = observer;
    let part = part_path(&spec.dest);
    let mut attempts = Attempts {
        without_progress: 0,
        peak_len: part_len(&part).await?,
        restarted_after_bad_range: false,
    };
    loop {
        let attempt = transfer_once(fetcher, spec, auth(), cancel, on_bytes).await;
        match resolve_attempt(attempt, &part, &mut attempts).await? {
            TransferOutcome::Interrupted { .. } => {}
            TransferOutcome::Restart => on_restart(),
            done => return Ok(done),
        }
        attempts.record(part_len(&part).await?);
        if policy.is_exhausted(&attempts) {
            return Err(DownloadError::Failed(FailureKind::Network));
        }
        if let Some(delay) = policy.delay_after(&attempts)
            && sleep_or_cancel(sleeper, delay, cancel).await
        {
            return Ok(TransferOutcome::Cancelled);
        }
    }
}

/// Un `Content-Range` incoherente con datos en el `.part` se trata una sola
/// vez como `Restart`. Con el `.part` vacío no se envía `Range`, así que
/// repetir la petición no cambiaría nada: es un fallo definitivo.
async fn resolve_attempt(
    attempt: Result<TransferOutcome, DownloadError>,
    part: &Path,
    attempts: &mut Attempts,
) -> Result<TransferOutcome, DownloadError> {
    let Err(DownloadError::Failed(FailureKind::UnexpectedContentRange)) = attempt else {
        return attempt;
    };
    if attempts.restarted_after_bad_range || part_len(part).await? == 0 {
        return attempt;
    }
    attempts.restarted_after_bad_range = true;
    reset_part(part).await?;
    Ok(TransferOutcome::Restart)
}

/// Devuelve `true` si la espera terminó por cancelación.
async fn sleep_or_cancel(
    sleeper: &dyn Sleeper,
    delay: Duration,
    cancel: &CancellationToken,
) -> bool {
    tokio::select! {
        biased;
        () = cancel.cancelled() => true,
        () = sleeper.sleep(delay) => cancel.is_cancelled(),
    }
}

#[cfg(test)]
pub use fakes::FakeSleeper;

#[cfg(test)]
mod fakes {
    use std::sync::Mutex;
    use std::time::Duration;

    use async_trait::async_trait;

    use super::Sleeper;

    /// Registra las esperas pedidas y vuelve al momento.
    #[derive(Default)]
    pub struct FakeSleeper {
        slept: Mutex<Vec<Duration>>,
    }

    impl FakeSleeper {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn slept_secs(&self) -> Vec<u64> {
            let slept = self.slept.lock().expect("registro de esperas");
            slept.iter().map(Duration::as_secs).collect()
        }
    }

    #[async_trait]
    impl Sleeper for FakeSleeper {
        async fn sleep(&self, duration: Duration) {
            self.slept
                .lock()
                .expect("registro de esperas")
                .push(duration);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use reqwest::Url;
    use wiremock::matchers::{header_exists, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::download::auth::auth_source;
    use crate::download::model::FailureKind;
    use crate::download::paths::part_path;
    use crate::download::test_support::{
        FakeTokens, closed_port, cutting_server, file_spec, hf_token, pattern_bytes,
    };
    use crate::hf::HfToken;

    const KIB: usize = 1024;
    const MIB: usize = 1024 * KIB;

    fn fetcher() -> HttpFetcher {
        HttpFetcher::new(reqwest::Client::new(), None)
    }

    fn dest_in(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("m.gguf")
    }

    fn seed_part(dest: &Path, bytes: &[u8]) {
        std::fs::write(part_path(dest), bytes).expect("escribir .part");
    }

    async fn run_with(
        fetcher: &HttpFetcher,
        spec: &FileSpec,
        sleeper: &dyn Sleeper,
        cancel: &CancellationToken,
    ) -> Result<TransferOutcome, DownloadError> {
        let policy = RetryPolicy::default();
        transfer_with_retry(
            fetcher,
            spec,
            &|| None,
            &policy,
            sleeper,
            cancel,
            TransferObserver {
                on_bytes: &mut |_| {},
                on_restart: &mut || {},
            },
        )
        .await
    }

    async fn run(spec: &FileSpec, sleeper: &FakeSleeper) -> Result<TransferOutcome, DownloadError> {
        run_with(&fetcher(), spec, sleeper, &CancellationToken::new()).await
    }

    fn failure(result: Result<TransferOutcome, DownloadError>) -> FailureKind {
        match result {
            Err(DownloadError::Failed(kind)) => kind,
            other => panic!("se esperaba Failed(_), llegó {other:?}"),
        }
    }

    async fn request_count(server: &MockServer) -> usize {
        server.received_requests().await.expect("peticiones").len()
    }

    async fn serve_status(status: u16) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn cortes_con_progreso_terminan() {
        let body = pattern_bytes(3 * MIB);
        let server = cutting_server(body.clone(), vec![Some(MIB), Some(MIB), None], false).await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        let spec = file_spec(server.url(), &dest, body.len());
        let sleeper = FakeSleeper::new();

        let outcome = run(&spec, &sleeper).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        assert_eq!(std::fs::read(part_path(&dest)).expect("leer"), body);
        let expected = [None, Some("bytes=1048576-"), Some("bytes=2097152-")]
            .map(|range| range.map(str::to_string));
        assert_eq!(server.ranges(), expected);
    }

    #[tokio::test]
    async fn cortes_sin_progreso_agotan() {
        let server = cutting_server(pattern_bytes(MIB), vec![Some(0); 5], false).await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        let spec = file_spec(server.url(), &dest, MIB);
        let sleeper = FakeSleeper::new();

        let outcome = run(&spec, &sleeper).await;

        assert_eq!(failure(outcome), FailureKind::Network);
        assert_eq!(server.ranges().len(), 5);
        assert_eq!(sleeper.slept_secs(), [1, 2, 4, 8]);
        assert!(part_path(&dest).is_file());
    }

    #[tokio::test]
    async fn progreso_reinicia_contador() {
        let body = pattern_bytes(MIB);
        let mut cuts = vec![Some(0); 4];
        cuts.push(Some(100 * KIB));
        cuts.extend(vec![Some(0); 4]);
        cuts.push(None);
        let server = cutting_server(body.clone(), cuts, false).await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        let spec = file_spec(server.url(), &dest, body.len());
        let sleeper = FakeSleeper::new();

        let outcome = run(&spec, &sleeper).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        assert_eq!(server.ranges().len(), 10);
        assert_eq!(std::fs::read(part_path(&dest)).expect("leer"), body);
    }

    #[tokio::test]
    async fn servidor_anuncia_menos_falla_sin_reintentar() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(pattern_bytes(900)))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        let spec = file_spec(format!("{}/f", server.uri()), &dest, 1000);
        let sleeper = FakeSleeper::new();

        let outcome = run(&spec, &sleeper).await;

        assert_eq!(
            failure(outcome),
            FailureKind::SizeMismatch {
                expected: 1000,
                actual: 900
            }
        );
        assert_eq!(request_count(&server).await, 1);
        assert!(sleeper.slept_secs().is_empty());
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn no_reintenta_401() {
        let server = serve_status(401).await;
        let dir = tempfile::tempdir().expect("tmp");
        let spec = file_spec(format!("{}/f", server.uri()), &dest_in(&dir), 10);
        let sleeper = FakeSleeper::new();

        let outcome = run(&spec, &sleeper).await;

        assert_eq!(failure(outcome), FailureKind::Unauthorized);
        assert_eq!(request_count(&server).await, 1);
    }

    #[tokio::test]
    async fn reintenta_503() {
        let body = pattern_bytes(KIB);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let spec = file_spec(format!("{}/f", server.uri()), &dest_in(&dir), body.len());
        let sleeper = FakeSleeper::new();

        let outcome = run(&spec, &sleeper).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        assert_eq!(sleeper.slept_secs(), [1, 2]);
    }

    /// Con `Range` responde un 206 cuyo inicio no es el pedido; sin `Range`,
    /// responde `second`.
    async fn serve_incoherent_then(second: ResponseTemplate) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header_exists("range"))
            .respond_with(incoherent_partial())
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(second)
            .mount(&server)
            .await;
        server
    }

    fn incoherent_partial() -> ResponseTemplate {
        ResponseTemplate::new(206)
            .insert_header("content-range", "bytes 5-262143/262144")
            .set_body_bytes(pattern_bytes(KIB))
    }

    #[tokio::test]
    async fn content_range_incoherente_reinicia_una_vez() {
        let body = pattern_bytes(256 * KIB);
        let server =
            serve_incoherent_then(ResponseTemplate::new(200).set_body_bytes(body.clone())).await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        seed_part(&dest, &body[..100 * KIB]);
        let spec = file_spec(format!("{}/f", server.uri()), &dest, body.len());
        let sleeper = FakeSleeper::new();

        let outcome = run(&spec, &sleeper).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        let requests = server.received_requests().await.expect("peticiones");
        assert_eq!(requests.len(), 2);
        assert!(!requests[1].headers.contains_key("range"));
        assert_eq!(std::fs::read(part_path(&dest)).expect("leer"), body);
    }

    #[tokio::test]
    async fn content_range_incoherente_dos_veces_falla() {
        let server = serve_incoherent_then(incoherent_partial()).await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        seed_part(&dest, &pattern_bytes(100 * KIB));
        let spec = file_spec(format!("{}/f", server.uri()), &dest, 256 * KIB);
        let sleeper = FakeSleeper::new();

        let outcome = run(&spec, &sleeper).await;

        assert_eq!(failure(outcome), FailureKind::UnexpectedContentRange);
        assert_eq!(request_count(&server).await, 2);
    }

    #[tokio::test]
    async fn reinicios_repetidos_acaban_fallando() {
        let body = pattern_bytes(3 * MIB);
        let server = cutting_server(body.clone(), vec![Some(MIB); 20], true).await;
        let dir = tempfile::tempdir().expect("tmp");
        let spec = file_spec(server.url(), &dest_in(&dir), body.len());
        let sleeper = FakeSleeper::new();

        let outcome = run(&spec, &sleeper).await;

        assert_eq!(failure(outcome), FailureKind::Network);
        assert_eq!(server.ranges().len(), 6);
        assert_eq!(sleeper.slept_secs(), [1, 2, 4, 8]);
    }

    #[tokio::test]
    async fn respuesta_416_tras_reinicio_falla() {
        let server = serve_status(416).await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        seed_part(&dest, &pattern_bytes(100 * KIB));
        let spec = file_spec(format!("{}/f", server.uri()), &dest, 256 * KIB);
        let sleeper = FakeSleeper::new();

        let outcome = run(&spec, &sleeper).await;

        assert_eq!(failure(outcome), FailureKind::UnexpectedContentRange);
        let requests = server.received_requests().await.expect("peticiones");
        assert_eq!(requests.len(), 2);
        assert!(requests[0].headers.contains_key("range"));
        assert!(!requests[1].headers.contains_key("range"));
        assert_eq!(sleeper.slept_secs(), [1]);
    }

    #[tokio::test]
    async fn sin_origen_hf_401_sin_reintento() {
        let server = serve_status(401).await;
        let dir = tempfile::tempdir().expect("tmp");
        let spec = file_spec(format!("{}/f", server.uri()), &dest_in(&dir), 10);
        let sleeper = FakeSleeper::new();

        let outcome = run(&spec, &sleeper).await;

        assert_eq!(failure(outcome), FailureKind::Unauthorized);
        assert_eq!(request_count(&server).await, 1);
        assert!(sleeper.slept_secs().is_empty());
    }

    #[tokio::test]
    async fn cdn_403_tras_redireccion_reintenta() {
        let cdn = serve_status(403).await;
        let hf = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", format!("{}/f", cdn.uri())),
            )
            .mount(&hf)
            .await;
        let hf_origin = Url::parse(&hf.uri()).expect("url");
        let fetcher = HttpFetcher::new(reqwest::Client::new(), Some(hf_origin));
        let dir = tempfile::tempdir().expect("tmp");
        let spec = file_spec(format!("{}/f", hf.uri()), &dest_in(&dir), 10);
        let sleeper = FakeSleeper::new();

        let outcome = run_with(&fetcher, &spec, &sleeper, &CancellationToken::new()).await;

        assert_eq!(failure(outcome), FailureKind::Network);
        assert_eq!(request_count(&hf).await, 5);
        assert_eq!(sleeper.slept_secs(), [1, 2, 4, 8]);
    }

    #[tokio::test]
    async fn conexion_rechazada_es_transitoria() {
        let url = format!("http://127.0.0.1:{}/f", closed_port());
        let dir = tempfile::tempdir().expect("tmp");
        let spec = file_spec(url, &dest_in(&dir), 10);
        let sleeper = FakeSleeper::new();

        let outcome = run(&spec, &sleeper).await;

        assert_eq!(failure(outcome), FailureKind::Network);
        assert_eq!(sleeper.slept_secs(), [1, 2, 4, 8]);
    }

    #[tokio::test]
    async fn fallo_red_agotado_no_filtra_url_firmada() {
        let port = closed_port();
        let url = format!("http://127.0.0.1:{port}/x?X-Amz-Signature=secreto123");
        let dir = tempfile::tempdir().expect("tmp");
        let spec = file_spec(url, &dest_in(&dir), 10);
        let sleeper = FakeSleeper::new();

        let error = run(&spec, &sleeper).await.expect_err("sin red");

        let DownloadError::Failed(kind) = &error else {
            panic!("se esperaba Failed(_), llegó {error:?}");
        };
        assert_eq!(*kind, FailureKind::Network);
        let rendered = format!(
            "{error} {error:?} {}",
            serde_json::to_string(kind).expect("json")
        );
        for leak in ["Signature", "secreto123", &format!("127.0.0.1:{port}/x")] {
            assert!(!rendered.contains(leak), "{leak} en {rendered}");
        }
    }

    struct CancellingSleeper(CancellationToken);

    #[async_trait]
    impl Sleeper for CancellingSleeper {
        async fn sleep(&self, _duration: Duration) {
            self.0.cancel();
        }
    }

    #[tokio::test]
    async fn cancel_durante_backoff() {
        let server = serve_status(503).await;
        let dir = tempfile::tempdir().expect("tmp");
        let spec = file_spec(format!("{}/f", server.uri()), &dest_in(&dir), 10);
        let cancel = CancellationToken::new();
        let sleeper = CancellingSleeper(cancel.clone());

        let outcome = run_with(&fetcher(), &spec, &sleeper, &cancel).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Cancelled)));
        assert_eq!(request_count(&server).await, 1);
    }

    /// Cambia el token del proveedor cada vez que el reintento duerme.
    struct TokenSwappingSleeper {
        tokens: Arc<FakeTokens>,
        replacement: Option<HfToken>,
    }

    #[async_trait]
    impl Sleeper for TokenSwappingSleeper {
        async fn sleep(&self, _duration: Duration) {
            self.tokens.set(self.replacement.clone());
        }
    }

    async fn authorizations_after_503_then_swap(
        replacement: Option<HfToken>,
    ) -> Vec<Option<String>> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(pattern_bytes(KIB)))
            .mount(&server)
            .await;
        let tokens = Arc::new(FakeTokens::new(Some(hf_token('A'))));
        let sleeper = TokenSwappingSleeper {
            tokens: Arc::clone(&tokens),
            replacement,
        };
        let dir = tempfile::tempdir().expect("tmp");
        let url = format!("{}/f", server.uri());
        let spec = file_spec(url.clone(), &dest_in(&dir), KIB);
        let hf_origin = Url::parse(&server.uri()).expect("url");
        let fetcher = HttpFetcher::new(reqwest::Client::new(), Some(hf_origin.clone()));
        let auth = auth_source(&url, &hf_origin, tokens.as_ref());

        let outcome = transfer_with_retry(
            &fetcher,
            &spec,
            &auth,
            &RetryPolicy::default(),
            &sleeper,
            &CancellationToken::new(),
            TransferObserver {
                on_bytes: &mut |_| {},
                on_restart: &mut || {},
            },
        )
        .await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        let requests = server.received_requests().await.expect("peticiones");
        requests
            .iter()
            .map(|request| {
                request
                    .headers
                    .get("authorization")
                    .map(|value| value.to_str().expect("ascii").to_string())
            })
            .collect()
    }

    fn bearer(tag: char) -> Option<String> {
        Some(format!("Bearer {}", hf_token(tag).expose()))
    }

    #[tokio::test]
    async fn token_se_relee_en_cada_intento() {
        let seen = authorizations_after_503_then_swap(Some(hf_token('B'))).await;

        assert_eq!(seen, [bearer('A'), bearer('B')]);
    }

    #[tokio::test]
    async fn token_borrado_entre_intentos() {
        let seen = authorizations_after_503_then_swap(None).await;

        assert_eq!(seen, [bearer('A'), None]);
    }

    #[derive(Debug, PartialEq)]
    enum Event {
        Restart,
        Bytes(u64),
    }

    /// Ejecuta la transferencia anotando, en orden, los bytes y los avisos de reinicio.
    async fn run_recording(
        spec: &FileSpec,
    ) -> (Result<TransferOutcome, DownloadError>, Vec<Event>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let bytes_log = Arc::clone(&events);
        let restart_log = Arc::clone(&events);
        let outcome = transfer_with_retry(
            &fetcher(),
            spec,
            &|| None,
            &RetryPolicy::default(),
            &FakeSleeper::new(),
            &CancellationToken::new(),
            TransferObserver {
                on_bytes: &mut move |bytes| {
                    bytes_log
                        .lock()
                        .expect("registro")
                        .push(Event::Bytes(bytes));
                },
                on_restart: &mut move || {
                    restart_log.lock().expect("registro").push(Event::Restart);
                },
            },
        )
        .await;
        let events = std::mem::take(&mut *events.lock().expect("registro"));
        (outcome, events)
    }

    fn restarts(events: &[Event]) -> usize {
        events
            .iter()
            .filter(|event| **event == Event::Restart)
            .count()
    }

    #[tokio::test]
    async fn restart_avisa_on_restart() {
        let body = pattern_bytes(256 * KIB);
        let server = cutting_server(body.clone(), Vec::new(), true).await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        seed_part(&dest, &body[..100 * KIB]);
        let spec = file_spec(server.url(), &dest, body.len());

        let (outcome, events) = run_recording(&spec).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        assert_eq!(restarts(&events), 1);
        let notice = events
            .iter()
            .position(|e| *e == Event::Restart)
            .expect("aviso");
        let after: u64 = events[notice + 1..]
            .iter()
            .map(|event| match event {
                Event::Bytes(bytes) => *bytes,
                Event::Restart => 0,
            })
            .sum();
        assert!(events[..notice].iter().all(|e| *e == Event::Restart));
        assert_eq!(after, 262_144);
    }

    #[tokio::test]
    async fn restart_por_content_range_avisa() {
        let body = pattern_bytes(256 * KIB);
        let server =
            serve_incoherent_then(ResponseTemplate::new(200).set_body_bytes(body.clone())).await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        seed_part(&dest, &body[..100 * KIB]);
        let spec = file_spec(format!("{}/f", server.uri()), &dest, body.len());

        let (outcome, events) = run_recording(&spec).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        assert_eq!(restarts(&events), 1);
    }

    #[tokio::test]
    async fn interrupcion_no_avisa_restart() {
        let body = pattern_bytes(3 * MIB);
        let server = cutting_server(body.clone(), vec![Some(MIB), Some(MIB), None], false).await;
        let dir = tempfile::tempdir().expect("tmp");
        let spec = file_spec(server.url(), &dest_in(&dir), body.len());

        let (outcome, events) = run_recording(&spec).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        assert_eq!(restarts(&events), 0);
    }
}
