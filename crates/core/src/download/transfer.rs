//! Transferencia de un fichero a su `.part`, con `Range` y cancelación.

use std::path::Path;

use reqwest::header::{AUTHORIZATION, CONTENT_RANGE, HeaderValue, RANGE};
use reqwest::{Response, Url};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use super::error::DownloadError;
use super::http::{ResponseAction, ResponseInfo, classify, range_header};
use super::model::{FailureKind, FileSpec};
use super::paths::part_path;

pub struct HttpFetcher {
    client: reqwest::Client,
    hf_origin: Option<Url>,
}

impl HttpFetcher {
    pub fn new(client: reqwest::Client, hf_origin: Option<Url>) -> Self {
        Self { client, hf_origin }
    }

    async fn request(
        &self,
        url: &str,
        offset: u64,
        auth: Option<HeaderValue>,
    ) -> Result<Response, reqwest::Error> {
        let mut request = self.client.get(url);
        if let Some(range) = range_header(offset) {
            request = request.header(RANGE, range);
        }
        if let Some(auth) = auth {
            request = request.header(AUTHORIZATION, auth);
        }
        request.send().await
    }

    /// Solo se afirma que la URL final es ajena a HF cuando ambos orígenes son
    /// conocidos y distintos; ante la duda, un 401/403 no se reintenta.
    fn is_hf_origin(&self, final_url: &Url) -> bool {
        let Some(hf_origin) = &self.hf_origin else {
            return true;
        };
        let (hf, last) = (hf_origin.origin(), final_url.origin());
        !(hf.is_tuple() && last.is_tuple() && hf != last)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferOutcome {
    Completed,
    Cancelled,
    Interrupted { bytes_written: u64 },
    Restart,
}

/// Punto único de conversión de un error de `reqwest` a un fallo de descarga.
/// El `Display` de `reqwest::Error` incluye la URL, que puede ser la URL
/// firmada de la CDN: el error se descarta y no se propaga ningún texto suyo.
pub fn network_failure(error: reqwest::Error) -> DownloadError {
    drop(error);
    DownloadError::Failed(FailureKind::Network)
}

/// Vacía el `.part` para empezar de cero.
pub async fn reset_part(part: &Path) -> Result<(), DownloadError> {
    tokio::fs::File::create(part).await?;
    Ok(())
}

pub async fn transfer_once(
    fetcher: &HttpFetcher,
    spec: &FileSpec,
    auth: Option<HeaderValue>,
    cancel: &CancellationToken,
    on_bytes: &mut (dyn FnMut(u64) + Send),
) -> Result<TransferOutcome, DownloadError> {
    let part = part_path(&spec.dest);
    let result = attempt(fetcher, spec, &part, auth, cancel, on_bytes).await;
    if matches!(
        result,
        Err(DownloadError::Failed(FailureKind::SizeMismatch { .. }))
    ) {
        remove_if_exists(&part).await?;
    }
    result
}

async fn attempt(
    fetcher: &HttpFetcher,
    spec: &FileSpec,
    part: &Path,
    auth: Option<HeaderValue>,
    cancel: &CancellationToken,
    on_bytes: &mut (dyn FnMut(u64) + Send),
) -> Result<TransferOutcome, DownloadError> {
    if let Some(parent) = spec.dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let offset = part_len(part).await?;
    let response = tokio::select! {
        biased;
        () = cancel.cancelled() => return Ok(TransferOutcome::Cancelled),
        sent = fetcher.request(&spec.url, offset, auth) => match sent {
            Ok(response) => response,
            Err(error) => return transport_error(error, 0),
        },
    };
    match classify_response(fetcher, &response, offset, spec.expected_size) {
        ResponseAction::Append => {
            receive(response, part, offset, spec.expected_size, cancel, on_bytes).await
        }
        ResponseAction::RestartFromZero => {
            reset_part(part).await?;
            Ok(TransferOutcome::Restart)
        }
        ResponseAction::AlreadyComplete => Ok(TransferOutcome::Completed),
        ResponseAction::Retry => Ok(TransferOutcome::Interrupted { bytes_written: 0 }),
        ResponseAction::Fail(kind) => Err(DownloadError::Failed(kind)),
    }
}

fn classify_response(
    fetcher: &HttpFetcher,
    response: &Response,
    offset: u64,
    expected: u64,
) -> ResponseAction {
    let content_range = response
        .headers()
        .get(CONTENT_RANGE)
        .and_then(|value| value.to_str().ok());
    let info = ResponseInfo {
        status: response.status().as_u16(),
        content_range,
        content_length: response.content_length(),
        from_hf_origin: fetcher.is_hf_origin(response.url()),
    };
    classify(info, offset, expected)
}

/// Un fallo de transporte se reintenta, salvo una petición mal formada.
fn transport_error(
    error: reqwest::Error,
    bytes_written: u64,
) -> Result<TransferOutcome, DownloadError> {
    if error.is_builder() {
        return Err(network_failure(error));
    }
    Ok(TransferOutcome::Interrupted { bytes_written })
}

pub(super) async fn part_len(part: &Path) -> Result<u64, DownloadError> {
    match tokio::fs::metadata(part).await {
        Ok(metadata) => Ok(metadata.len()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

pub(super) async fn remove_if_exists(part: &Path) -> Result<(), DownloadError> {
    match tokio::fs::remove_file(part).await {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

async fn receive(
    mut response: Response,
    part: &Path,
    offset: u64,
    expected: u64,
    cancel: &CancellationToken,
    on_bytes: &mut (dyn FnMut(u64) + Send),
) -> Result<TransferOutcome, DownloadError> {
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(part)
        .await?;
    let outcome = copy_body(&mut response, &mut file, offset, expected, cancel, on_bytes).await;
    // Se vuelca también al cancelar o fallar: el `.part` es la fuente de verdad.
    let flushed = file.flush().await;
    let outcome = outcome?;
    flushed?;
    Ok(outcome)
}

async fn copy_body(
    response: &mut Response,
    file: &mut tokio::fs::File,
    offset: u64,
    expected: u64,
    cancel: &CancellationToken,
    on_bytes: &mut (dyn FnMut(u64) + Send),
) -> Result<TransferOutcome, DownloadError> {
    let mut total = offset;
    let mut written = 0u64;
    loop {
        let chunk = tokio::select! {
            biased;
            () = cancel.cancelled() => return Ok(TransferOutcome::Cancelled),
            next = response.chunk() => match next {
                Ok(Some(chunk)) => chunk,
                Ok(None) => break,
                Err(error) => return transport_error(error, written),
            },
        };
        let len = chunk.len() as u64;
        // Se corta antes de escribir el exceso para no llenar el disco.
        if total.saturating_add(len) > expected {
            return Err(DownloadError::Failed(FailureKind::SizeMismatch {
                expected,
                actual: total.saturating_add(len),
            }));
        }
        file.write_all(&chunk).await?;
        total += len;
        written += len;
        on_bytes(len);
    }
    Ok(if total == expected {
        TransferOutcome::Completed
    } else {
        TransferOutcome::Interrupted {
            bytes_written: written,
        }
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use reqwest::header::HeaderValue;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::download::model::FailureKind;
    use crate::download::paths::part_path;
    use crate::download::test_support::{closed_port, file_spec, pattern_bytes};

    const KIB: usize = 1024;

    fn fetcher() -> HttpFetcher {
        HttpFetcher::new(reqwest::Client::new(), None)
    }

    async fn run(
        fetcher: &HttpFetcher,
        spec: &FileSpec,
        cancel: &CancellationToken,
    ) -> Result<TransferOutcome, DownloadError> {
        transfer_once(fetcher, spec, None, cancel, &mut |_| {}).await
    }

    fn dest_in(dir: &tempfile::TempDir) -> PathBuf {
        dir.path().join("m.gguf")
    }

    fn seed_part(dest: &Path, bytes: &[u8]) {
        std::fs::write(part_path(dest), bytes).expect("escribir .part");
    }

    fn failure(result: Result<TransferOutcome, DownloadError>) -> FailureKind {
        match result {
            Err(DownloadError::Failed(kind)) => kind,
            other => panic!("se esperaba Failed(_), llegó {other:?}"),
        }
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
    async fn descarga_completa_desde_cero() {
        let body = pattern_bytes(256 * KIB);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/f"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        let spec = file_spec(format!("{}/f", server.uri()), &dest, body.len());

        let outcome = run(&fetcher(), &spec, &CancellationToken::new()).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        assert_eq!(std::fs::read(part_path(&dest)).expect("leer"), body);
        let requests = server.received_requests().await.expect("peticiones");
        assert!(!requests[0].headers.contains_key("range"));
    }

    #[tokio::test]
    async fn reanuda_con_range() {
        let body = pattern_bytes(256 * KIB);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("range", "bytes=102400-"))
            .respond_with(
                ResponseTemplate::new(206)
                    .insert_header("content-range", "bytes 102400-262143/262144")
                    .set_body_bytes(body[100 * KIB..].to_vec()),
            )
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        seed_part(&dest, &body[..100 * KIB]);
        let spec = file_spec(format!("{}/f", server.uri()), &dest, body.len());

        let outcome = run(&fetcher(), &spec, &CancellationToken::new()).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        assert_eq!(std::fs::read(part_path(&dest)).expect("leer"), body);
    }

    #[tokio::test]
    async fn servidor_ignora_range_trunca() {
        let body = pattern_bytes(256 * KIB);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        seed_part(&dest, &body[..100 * KIB]);
        let spec = file_spec(format!("{}/f", server.uri()), &dest, body.len());
        let cancel = CancellationToken::new();

        let first = run(&fetcher(), &spec, &cancel).await;
        assert!(matches!(first, Ok(TransferOutcome::Restart)));
        assert_eq!(std::fs::metadata(part_path(&dest)).expect("meta").len(), 0);

        let second = run(&fetcher(), &spec, &cancel).await;
        assert!(matches!(second, Ok(TransferOutcome::Completed)));
        assert_eq!(std::fs::read(part_path(&dest)).expect("leer"), body);
    }

    #[tokio::test]
    async fn range_416_ya_completo() {
        let body = pattern_bytes(4 * KIB);
        let server = serve_status(416).await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        seed_part(&dest, &body);
        let spec = file_spec(format!("{}/f", server.uri()), &dest, body.len());

        let outcome = run(&fetcher(), &spec, &CancellationToken::new()).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        assert_eq!(std::fs::read(part_path(&dest)).expect("leer"), body);
    }

    #[tokio::test]
    async fn content_range_incoherente_falla() {
        let body = pattern_bytes(256 * KIB);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(206)
                    .insert_header("content-range", "bytes 0-262143/262144")
                    .set_body_bytes(body.clone()),
            )
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        seed_part(&dest, &body[..100 * KIB]);
        let spec = file_spec(format!("{}/f", server.uri()), &dest, body.len());

        let outcome = run(&fetcher(), &spec, &CancellationToken::new()).await;

        assert_eq!(failure(outcome), FailureKind::UnexpectedContentRange);
    }

    #[tokio::test]
    async fn servidor_envia_de_mas_corta() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(pattern_bytes(1200)))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        let spec = file_spec(format!("{}/f", server.uri()), &dest, 1000);
        let mut peak = 0u64;

        let outcome = transfer_once(
            &fetcher(),
            &spec,
            None,
            &CancellationToken::new(),
            &mut |bytes| peak += bytes,
        )
        .await;

        assert!(matches!(
            failure(outcome),
            FailureKind::SizeMismatch { expected: 1000, .. }
        ));
        assert!(peak <= 1000);
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn servidor_416_total_distinto_falla() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(416).insert_header("content-range", "bytes */2000"))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        seed_part(&dest, &pattern_bytes(100));
        let spec = file_spec(format!("{}/f", server.uri()), &dest, 1000);

        let outcome = run(&fetcher(), &spec, &CancellationToken::new()).await;

        assert_eq!(
            failure(outcome),
            FailureKind::SizeMismatch {
                expected: 1000,
                actual: 2000
            }
        );
        assert_eq!(
            server.received_requests().await.expect("peticiones").len(),
            1
        );
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn cancelacion_entre_bloques() {
        let body = pattern_bytes(1024 * KIB);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        let spec = file_spec(format!("{}/f", server.uri()), &dest, body.len());
        let cancel = CancellationToken::new();
        let mut written_at_cancel = 0u64;

        let outcome = transfer_once(&fetcher(), &spec, None, &cancel, &mut |bytes| {
            written_at_cancel += bytes;
            cancel.cancel();
        })
        .await;

        assert!(matches!(outcome, Ok(TransferOutcome::Cancelled)));
        let part = std::fs::read(part_path(&dest)).expect("leer");
        assert_eq!(part.len() as u64, written_at_cancel);
        assert_eq!(part, body[..part.len()]);
    }

    #[tokio::test]
    async fn cancelacion_durante_espera_de_respuesta() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(60)))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        let spec = file_spec(format!("{}/f", server.uri()), &dest, 10);
        let cancel = CancellationToken::new();
        let fetcher = fetcher();

        let canceller = async {
            while server
                .received_requests()
                .await
                .expect("peticiones")
                .is_empty()
            {
                tokio::task::yield_now().await;
            }
            cancel.cancel();
        };
        let (outcome, ()) = tokio::join!(run(&fetcher, &spec, &cancel), canceller);

        assert!(matches!(outcome, Ok(TransferOutcome::Cancelled)));
    }

    #[tokio::test]
    async fn sigue_redireccion() {
        let body = pattern_bytes(8 * KIB);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/a"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/b"))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/b"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        let spec = file_spec(format!("{}/a", server.uri()), &dest, body.len());

        let outcome = run(&fetcher(), &spec, &CancellationToken::new()).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        assert_eq!(std::fs::read(part_path(&dest)).expect("leer"), body);
    }

    #[tokio::test]
    async fn respuesta_404() {
        let server = serve_status(404).await;
        let dir = tempfile::tempdir().expect("tmp");
        let spec = file_spec(format!("{}/f", server.uri()), &dest_in(&dir), 10);

        let outcome = run(&fetcher(), &spec, &CancellationToken::new()).await;

        assert_eq!(failure(outcome), FailureKind::NotFound);
    }

    #[tokio::test]
    async fn sin_origen_hf_401_es_unauthorized() {
        let server = serve_status(401).await;
        let dir = tempfile::tempdir().expect("tmp");
        let spec = file_spec(format!("{}/f", server.uri()), &dest_in(&dir), 10);

        let outcome = run(&fetcher(), &spec, &CancellationToken::new()).await;

        assert_eq!(failure(outcome), FailureKind::Unauthorized);
    }

    #[tokio::test]
    async fn sin_origen_hf_403_es_gated() {
        let server = serve_status(403).await;
        let dir = tempfile::tempdir().expect("tmp");
        let spec = file_spec(format!("{}/f", server.uri()), &dest_in(&dir), 10);

        let outcome = run(&fetcher(), &spec, &CancellationToken::new()).await;

        assert_eq!(failure(outcome), FailureKind::Gated);
    }

    #[tokio::test]
    async fn crea_directorio_padre() {
        let body = pattern_bytes(KIB);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body.clone()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("x").join("y").join("m.gguf");
        let spec = file_spec(format!("{}/f", server.uri()), &dest, body.len());

        let outcome = run(&fetcher(), &spec, &CancellationToken::new()).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        assert!(part_path(&dest).is_file());
    }

    #[tokio::test]
    async fn envia_la_autorizacion_recibida() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("authorization", "Bearer abc"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(pattern_bytes(10)))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dest_in(&dir);
        let spec = file_spec(format!("{}/f", server.uri()), &dest, 10);
        let auth = HeaderValue::from_static("Bearer abc");

        let outcome = transfer_once(
            &fetcher(),
            &spec,
            Some(auth),
            &CancellationToken::new(),
            &mut |_| {},
        )
        .await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
    }

    #[tokio::test]
    async fn origen_distinto_al_de_hf_trata_403_como_transitorio() {
        let server = serve_status(403).await;
        let other_origin = Url::parse("http://127.0.0.1:9").expect("url");
        let fetcher = HttpFetcher::new(reqwest::Client::new(), Some(other_origin));
        let dir = tempfile::tempdir().expect("tmp");
        let spec = file_spec(format!("{}/f", server.uri()), &dest_in(&dir), 10);

        let outcome = run(&fetcher, &spec, &CancellationToken::new()).await;

        assert!(matches!(
            outcome,
            Ok(TransferOutcome::Interrupted { bytes_written: 0 })
        ));
    }

    #[tokio::test]
    async fn error_reqwest_no_filtra_url_firmada() {
        let port = closed_port();
        let url = format!("http://127.0.0.1:{port}/x?X-Amz-Signature=secreto123");
        let error = reqwest::Client::new()
            .get(&url)
            .send()
            .await
            .expect_err("sin red");

        let converted = network_failure(error);

        let DownloadError::Failed(kind) = &converted else {
            panic!("se esperaba Failed(_), llegó {converted:?}");
        };
        let rendered = format!(
            "{converted} {converted:?} {}",
            serde_json::to_string(kind).expect("json")
        );
        for leak in ["Signature", "secreto123", &format!("127.0.0.1:{port}/x")] {
            assert!(!rendered.contains(leak), "{leak} en {rendered}");
        }
    }
}
