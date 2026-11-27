//! Descarga de artefactos con verificación de tamaño y SHA256.
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::runtime::RuntimeError;

#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub url: String,
    pub dest: PathBuf,
    pub expected_size: u64,
    pub sha256: Option<[u8; 32]>,
}

/// Descarga un artefacto a `req.dest`; el fichero final solo existe si la descarga es íntegra.
///
/// El plan 006 aportará una implementación con reanudación.
#[async_trait]
pub trait ArtifactFetcher: Send + Sync {
    async fn fetch(
        &self,
        req: FetchRequest,
        progress: &(dyn Fn(u64, u64) + Send + Sync),
        cancel: &CancelToken,
    ) -> Result<(), RuntimeError>;
}

#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    fn check(&self) -> Result<(), RuntimeError> {
        if self.is_cancelled() {
            Err(RuntimeError::Cancelled)
        } else {
            Ok(())
        }
    }
}

pub struct SimpleHttpFetcher {
    client: reqwest::Client,
}

impl SimpleHttpFetcher {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    async fn download(
        &self,
        req: &FetchRequest,
        part: &Path,
        progress: &(dyn Fn(u64, u64) + Send + Sync),
        cancel: &CancelToken,
    ) -> Result<(), RuntimeError> {
        cancel.check()?;
        let response = self.client.get(&req.url).send().await?;
        if !response.status().is_success() {
            return Err(RuntimeError::HttpStatus(response.status().as_u16()));
        }
        let mut file = tokio::fs::File::create(part).await?;
        let mut hasher = Sha256::new();
        let mut done = 0u64;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            cancel.check()?;
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            hasher.update(&chunk);
            done += chunk.len() as u64;
            progress(done, req.expected_size);
        }
        file.flush().await?;
        verify(req, done, hasher.finalize().into())?;
        tokio::fs::rename(part, &req.dest).await?;
        Ok(())
    }
}

#[async_trait]
impl ArtifactFetcher for SimpleHttpFetcher {
    async fn fetch(
        &self,
        req: FetchRequest,
        progress: &(dyn Fn(u64, u64) + Send + Sync),
        cancel: &CancelToken,
    ) -> Result<(), RuntimeError> {
        let part = part_path(&req.dest);
        let result = self.download(&req, &part, progress, cancel).await;
        if result.is_err() {
            // Si el `.part` no llegó a crearse no hay nada que limpiar.
            let _ = tokio::fs::remove_file(&part).await;
        }
        result
    }
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_owned();
    name.push(".part");
    PathBuf::from(name)
}

fn verify(req: &FetchRequest, actual_size: u64, actual_hash: [u8; 32]) -> Result<(), RuntimeError> {
    if actual_size != req.expected_size {
        return Err(RuntimeError::SizeMismatch {
            expected: req.expected_size,
            actual: actual_size,
        });
    }
    match req.sha256 {
        Some(expected) if expected != actual_hash => Err(RuntimeError::IntegrityMismatch),
        _ => Ok(()),
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

    const MIB: usize = 1024 * 1024;

    fn content() -> Vec<u8> {
        (0..MIB).map(|i| (i % 251) as u8).collect()
    }

    fn sha_of(bytes: &[u8]) -> [u8; 32] {
        Sha256::digest(bytes).into()
    }

    fn part_of(dest: &Path) -> PathBuf {
        let mut name = dest.as_os_str().to_owned();
        name.push(".part");
        PathBuf::from(name)
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
        assert!(!part_of(dest).exists());
    }

    async fn run(req: FetchRequest, cancel: &CancelToken) -> Result<(), RuntimeError> {
        SimpleHttpFetcher::new(reqwest::Client::new())
            .fetch(req, &|_, _| {}, cancel)
            .await
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
        assert!(!part_of(&dest).exists());
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

        let result = run(req, &CancelToken::default()).await;

        assert!(matches!(
            result,
            Err(RuntimeError::SizeMismatch { expected, actual })
                if expected == MIB as u64 + 1 && actual == MIB as u64
        ));
        assert_no_files(&dest);
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

        SimpleHttpFetcher::new(reqwest::Client::new())
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

    #[test]
    fn cancel_token_compartido() {
        let token = CancelToken::default();
        let clone = token.clone();
        assert!(!clone.is_cancelled());

        token.cancel();

        assert!(clone.is_cancelled());
    }
}
