//! Descarga de artefactos con reintentos, reanudación y verificación de tamaño y SHA256.
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::download::{
    DownloadError, FailureKind, HttpFetcher, SingleRequest, Sleeper, download_one, part_path,
};
use crate::runtime::RuntimeError;

#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub url: String,
    pub dest: PathBuf,
    pub expected_size: u64,
    pub sha256: Option<[u8; 32]>,
}

/// Descarga un artefacto a `req.dest`; el fichero final solo existe si la descarga es íntegra.
#[async_trait]
pub trait ArtifactFetcher: Send + Sync {
    async fn fetch(
        &self,
        req: FetchRequest,
        progress: &(dyn Fn(u64, u64) + Send + Sync),
        cancel: &CancelToken,
    ) -> Result<(), RuntimeError>;
}

/// La cancelación llega también a una espera de reintento o a una conexión parada.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(CancellationToken);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.cancel();
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    pub(super) fn check(&self) -> Result<(), RuntimeError> {
        if self.is_cancelled() {
            Err(RuntimeError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub(crate) fn as_cancellation(&self) -> &CancellationToken {
        &self.0
    }
}

pub struct ResumableHttpFetcher {
    http: HttpFetcher,
    sleeper: Arc<dyn Sleeper>,
}

impl ResumableHttpFetcher {
    /// Sin origen de HF ni token: los artefactos del runtime no son de Hugging Face.
    pub fn new(client: reqwest::Client, sleeper: Arc<dyn Sleeper>) -> Self {
        Self {
            http: HttpFetcher::new(client, None),
            sleeper,
        }
    }
}

#[async_trait]
impl ArtifactFetcher for ResumableHttpFetcher {
    async fn fetch(
        &self,
        req: FetchRequest,
        progress: &(dyn Fn(u64, u64) + Send + Sync),
        cancel: &CancelToken,
    ) -> Result<(), RuntimeError> {
        let part = part_path(&req.dest);
        let single = SingleRequest {
            url: req.url,
            dest: req.dest,
            expected_size: req.expected_size,
            sha256: req.sha256.map(hex::encode),
        };
        let result = download_one(
            &self.http,
            single,
            self.sleeper.as_ref(),
            cancel.as_cancellation(),
            progress,
        )
        .await;
        if result.is_err() {
            // Si el `.part` no llegó a crearse no hay nada que limpiar.
            let _ = tokio::fs::remove_file(&part).await;
        }
        result.map_err(runtime_error)
    }
}

fn runtime_error(error: DownloadError) -> RuntimeError {
    match error {
        DownloadError::Failed(FailureKind::SizeMismatch { expected, actual }) => {
            RuntimeError::SizeMismatch { expected, actual }
        }
        DownloadError::Failed(FailureKind::ChecksumMismatch) => RuntimeError::IntegrityMismatch,
        DownloadError::Cancelled => RuntimeError::Cancelled,
        DownloadError::Failed(FailureKind::NotFound) => RuntimeError::HttpStatus(404),
        DownloadError::Failed(FailureKind::Unauthorized) => RuntimeError::HttpStatus(401),
        DownloadError::Failed(FailureKind::Gated) => RuntimeError::HttpStatus(403),
        DownloadError::Failed(FailureKind::Network | FailureKind::UnexpectedContentRange) => {
            RuntimeError::Network
        }
        DownloadError::Io(error) => RuntimeError::Io(error),
        other => RuntimeError::Io(std::io::Error::other(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    use sha2::{Digest, Sha256};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::download::FakeSleeper;
    use crate::download::test_support::closed_port;

    const MIB: usize = 1024 * 1024;

    fn content() -> Vec<u8> {
        (0..MIB).map(|i| (i % 251) as u8).collect()
    }

    fn sha_of(bytes: &[u8]) -> [u8; 32] {
        Sha256::digest(bytes).into()
    }

    async fn serve(body: Vec<u8>) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/asset"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&server)
            .await;
        server
    }

    fn request(
        server: &MockServer,
        dest: PathBuf,
        size: u64,
        sha256: Option<[u8; 32]>,
    ) -> FetchRequest {
        FetchRequest {
            url: format!("{}/asset", server.uri()),
            dest,
            expected_size: size,
            sha256,
        }
    }

    fn assert_no_files(dest: &Path) {
        assert!(!dest.exists());
        assert!(!part_path(dest).exists());
    }

    /// Servidor HTTP/1.1 de una sola respuesta con `Transfer-Encoding: chunked`.
    fn serve_chunked(chunks: Vec<Vec<u8>>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request);
            let mut response = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n".to_vec();
            for chunk in chunks {
                response.extend(format!("{:x}\r\n", chunk.len()).into_bytes());
                response.extend(chunk);
                response.extend(b"\r\n");
            }
            response.extend(b"0\r\n\r\n");
            let _ = stream.write_all(&response);
        });
        format!("http://{addr}/asset")
    }

    fn fetcher_with(sleeper: Arc<FakeSleeper>) -> ResumableHttpFetcher {
        ResumableHttpFetcher::new(reqwest::Client::new(), sleeper)
    }

    fn fetcher() -> ResumableHttpFetcher {
        fetcher_with(Arc::new(FakeSleeper::new()))
    }

    async fn run(req: FetchRequest, cancel: &CancelToken) -> Result<(), RuntimeError> {
        fetcher().fetch(req, &|_, _| {}, cancel).await
    }

    #[tokio::test]
    async fn fetch_ok_verifica_hash_y_tamano() {
        let body = content();
        let server = serve(body.clone()).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.zip");
        let req = request(&server, dest.clone(), MIB as u64, Some(sha_of(&body)));

        run(req, &CancelToken::default()).await.unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), body);
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn fetch_sin_digest_solo_tamano() {
        let server = serve(content()).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.zip");

        let req = request(&server, dest.clone(), MIB as u64, None);

        run(req, &CancelToken::default()).await.unwrap();
        assert!(dest.exists());
    }

    #[tokio::test]
    async fn fetch_hash_distinto() {
        let server = serve(content()).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.zip");
        let req = request(&server, dest.clone(), MIB as u64, Some(sha_of(b"otro")));

        let result = run(req, &CancelToken::default()).await;

        assert!(matches!(result, Err(RuntimeError::IntegrityMismatch)));
        assert_no_files(&dest);
    }

    #[tokio::test]
    async fn fetch_tamano_distinto() {
        let server = serve(content()).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.zip");
        let req = request(&server, dest.clone(), MIB as u64 + 1, None);
        let sleeper = Arc::new(FakeSleeper::new());

        let result = fetcher_with(sleeper.clone())
            .fetch(req, &|_, _| {}, &CancelToken::default())
            .await;

        assert!(matches!(
            result,
            Err(RuntimeError::SizeMismatch { expected, actual })
                if expected == MIB as u64 + 1 && actual == MIB as u64
        ));
        assert_no_files(&dest);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        assert!(sleeper.slept_secs().is_empty());
    }

    #[tokio::test]
    async fn fetch_http_404() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.zip");
        let req = request(&server, dest.clone(), 1, None);

        let result = run(req, &CancelToken::default()).await;

        assert!(matches!(result, Err(RuntimeError::HttpStatus(404))));
        assert_no_files(&dest);
    }

    #[tokio::test]
    async fn fetch_cancelado() {
        let server = serve(content()).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.zip");
        let req = request(&server, dest.clone(), MIB as u64, None);
        let cancel = CancelToken::default();
        cancel.cancel();

        let result = run(req, &cancel).await;

        assert!(matches!(result, Err(RuntimeError::Cancelled)));
        assert_no_files(&dest);
    }

    #[tokio::test]
    async fn fetch_progreso_monotono() {
        let server = serve(content()).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.zip");
        let req = request(&server, dest, MIB as u64, None);
        let calls = Mutex::new(Vec::new());

        fetcher()
            .fetch(
                req,
                &|done, total| calls.lock().unwrap().push((done, total)),
                &CancelToken::default(),
            )
            .await
            .unwrap();

        let calls = calls.into_inner().unwrap();
        assert!(calls.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert_eq!(calls.last(), Some(&(MIB as u64, MIB as u64)));
    }

    #[tokio::test]
    async fn fetch_sigue_redirecciones() {
        let server = serve(content()).await;
        Mock::given(method("GET"))
            .and(path("/release"))
            .respond_with(ResponseTemplate::new(302).insert_header("location", "/asset"))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.zip");
        let mut req = request(&server, dest.clone(), MIB as u64, None);
        req.url = format!("{}/release", server.uri());

        run(req, &CancelToken::default()).await.unwrap();

        assert!(dest.exists());
    }

    #[tokio::test]
    async fn fetch_tamano_excedido() {
        let server = serve(content()).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.zip");
        let req = request(&server, dest.clone(), MIB as u64 - 1, None);

        let result = run(req, &CancelToken::default()).await;

        assert!(matches!(
            result,
            Err(RuntimeError::SizeMismatch { expected, .. }) if expected == MIB as u64 - 1
        ));
        assert_no_files(&dest);
    }

    // Sin Content-Length (chunked) el exceso solo se detecta durante la descarga.
    #[tokio::test]
    async fn fetch_chunked_excedido() {
        let url = serve_chunked(vec![vec![1u8; 600], vec![2u8; 600]]);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.zip");
        let req = FetchRequest {
            url,
            dest: dest.clone(),
            expected_size: 1000,
            sha256: None,
        };

        let result = run(req, &CancelToken::default()).await;

        assert!(matches!(
            result,
            Err(RuntimeError::SizeMismatch { expected: 1000, actual }) if actual > 1000
        ));
        assert_no_files(&dest);
    }

    #[tokio::test]
    async fn fetch_cancelado_a_mitad_de_descarga() {
        let server = serve(content()).await;
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.zip");
        let req = request(&server, dest.clone(), MIB as u64, None);
        let cancel = CancelToken::default();

        let result = fetcher().fetch(req, &|_, _| cancel.cancel(), &cancel).await;

        assert!(matches!(result, Err(RuntimeError::Cancelled)));
        assert_no_files(&dest);
    }

    #[tokio::test]
    async fn fetch_corte_persistente_es_network() {
        let sleeper = Arc::new(FakeSleeper::new());
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("a.zip");
        let req = FetchRequest {
            url: format!("http://127.0.0.1:{}/asset", closed_port()),
            dest: dest.clone(),
            expected_size: MIB as u64,
            sha256: None,
        };

        let result = fetcher_with(Arc::clone(&sleeper))
            .fetch(req, &|_, _| {}, &CancelToken::default())
            .await;

        assert!(matches!(result, Err(RuntimeError::Network)));
        assert_eq!(sleeper.slept_secs(), [1, 2, 4, 8]);
        assert_no_files(&dest);
    }

    #[test]
    fn cancel_token_corta_la_espera() {
        let token = CancelToken::default();

        token.cancel();

        assert!(token.as_cancellation().is_cancelled());
    }

    #[test]
    fn cancel_token_compartido() {
        let token = CancelToken::default();
        let clone = token.clone();
        assert!(!clone.is_cancelled());

        token.cancel();

        assert!(clone.is_cancelled());
    }
}
