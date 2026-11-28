//! Descarga de un único fichero con reanudación, sin cola ni persistencia.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use tokio_util::sync::CancellationToken;

use super::error::DownloadError;
use super::model::{FailureKind, FileSpec, FileStatus};
use super::paths::part_path;
use super::retry::{RetryPolicy, Sleeper, TransferObserver, transfer_with_retry};
use super::transfer::{HttpFetcher, TransferOutcome, part_len, remove_if_exists};
use super::verify::verify_file;

#[derive(Debug, Clone)]
pub struct SingleRequest {
    pub url: String,
    pub dest: PathBuf,
    pub expected_size: u64,
    pub sha256: Option<String>,
}

impl SingleRequest {
    fn into_spec(self) -> FileSpec {
        FileSpec {
            relative_path: self
                .dest
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            url: self.url,
            dest: self.dest,
            expected_size: self.expected_size,
            sha256: self.sha256,
            status: FileStatus::Pending,
        }
    }
}

/// Descarga `req.url` a `req.dest`, que solo existe si la descarga es íntegra.
/// `on_progress` recibe `(descargado, total)` acumulados; tras un reinicio
/// vuelve a 0.
pub async fn download_one(
    fetcher: &HttpFetcher,
    req: SingleRequest,
    sleeper: &dyn Sleeper,
    cancel: &CancellationToken,
    on_progress: &(dyn Fn(u64, u64) + Send + Sync),
) -> Result<(), DownloadError> {
    let spec = req.into_spec();
    let part = part_path(&spec.dest);
    transfer(fetcher, &spec, sleeper, cancel, on_progress).await?;
    verify_part(&part, &spec, cancel).await?;
    tokio::fs::rename(&part, &spec.dest).await?;
    Ok(())
}

async fn transfer(
    fetcher: &HttpFetcher,
    spec: &FileSpec,
    sleeper: &dyn Sleeper,
    cancel: &CancellationToken,
    on_progress: &(dyn Fn(u64, u64) + Send + Sync),
) -> Result<(), DownloadError> {
    let total = spec.expected_size;
    let done = AtomicU64::new(part_len(&part_path(&spec.dest)).await?);
    let outcome = transfer_with_retry(
        fetcher,
        spec,
        &|| None,
        &RetryPolicy::default(),
        sleeper,
        cancel,
        TransferObserver {
            on_bytes: &mut |bytes| {
                let now = done.fetch_add(bytes, Ordering::Relaxed) + bytes;
                on_progress(now, total);
            },
            on_restart: &mut || {
                done.store(0, Ordering::Relaxed);
                on_progress(0, total);
            },
        },
    )
    .await?;
    match outcome {
        TransferOutcome::Completed => Ok(()),
        TransferOutcome::Cancelled => Err(DownloadError::Cancelled),
        // `transfer_with_retry` solo devuelve estos dos al agotar los intentos.
        TransferOutcome::Interrupted { .. } | TransferOutcome::Restart => {
            Err(DownloadError::Failed(FailureKind::Network))
        }
    }
}

/// Un tamaño o un hash incorrectos invalidan el `.part`: se borra para que el
/// siguiente intento no reanude sobre datos corruptos.
async fn verify_part(
    part: &Path,
    spec: &FileSpec,
    cancel: &CancellationToken,
) -> Result<(), DownloadError> {
    let result = verify_file(
        part,
        spec.expected_size,
        spec.sha256.as_deref(),
        cancel,
        |_| {},
    )
    .await;
    if matches!(
        result,
        Err(DownloadError::Failed(
            FailureKind::SizeMismatch { .. } | FailureKind::ChecksumMismatch
        ))
    ) {
        remove_if_exists(part).await?;
    }
    result
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use wiremock::matchers::{header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::download::test_support::{cutting_server, pattern_bytes};
    use crate::download::{FailureKind, FakeSleeper};

    const KIB: usize = 1024;

    fn fetcher() -> HttpFetcher {
        HttpFetcher::new(reqwest::Client::new(), None)
    }

    fn request(url: String, dest: PathBuf, size: usize, sha256: Option<String>) -> SingleRequest {
        SingleRequest {
            url,
            dest,
            expected_size: size as u64,
            sha256,
        }
    }

    async fn run(req: SingleRequest, sleeper: &FakeSleeper) -> Result<(), DownloadError> {
        download_one(
            &fetcher(),
            req,
            sleeper,
            &CancellationToken::new(),
            &|_, _| {},
        )
        .await
    }

    async fn serve(body: Vec<u8>) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn download_one_completa() {
        let body = pattern_bytes(512 * KIB);
        let server = serve(body.clone()).await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("a.bin");
        let req = request(server.uri(), dest.clone(), body.len(), None);

        run(req, &FakeSleeper::new()).await.expect("descarga");

        assert_eq!(std::fs::read(&dest).expect("leer"), body);
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn download_one_reanuda_part_previo() {
        let body = pattern_bytes(512 * KIB);
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("range", "bytes=102400-"))
            .respond_with(
                ResponseTemplate::new(206)
                    .insert_header("content-range", "bytes 102400-524287/524288")
                    .set_body_bytes(body[100 * KIB..].to_vec()),
            )
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("a.bin");
        std::fs::write(part_path(&dest), &body[..100 * KIB]).expect("sembrar .part");
        let req = request(server.uri(), dest.clone(), body.len(), None);

        run(req, &FakeSleeper::new()).await.expect("descarga");

        assert_eq!(std::fs::read(&dest).expect("leer"), body);
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn download_one_sha_incorrecto() {
        let body = pattern_bytes(512 * KIB);
        let server = serve(body.clone()).await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("a.bin");
        let wrong = Some("00".repeat(32));
        let req = request(server.uri(), dest.clone(), body.len(), wrong);

        let result = run(req, &FakeSleeper::new()).await;

        assert!(matches!(
            result,
            Err(DownloadError::Failed(FailureKind::ChecksumMismatch))
        ));
        assert!(!dest.exists());
        assert!(!part_path(&dest).exists());
    }

    #[tokio::test]
    async fn download_one_reintenta_corte() {
        let body = pattern_bytes(512 * KIB);
        let server = cutting_server(body.clone(), vec![Some(256 * KIB)], false).await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("a.bin");
        let req = request(server.url(), dest.clone(), body.len(), None);

        run(req, &FakeSleeper::new()).await.expect("descarga");

        assert_eq!(std::fs::read(&dest).expect("leer"), body);
        assert_eq!(server.ranges().len(), 2);
    }

    #[tokio::test]
    async fn download_one_informa_del_progreso_acumulado() {
        let body = pattern_bytes(512 * KIB);
        let server = serve(body.clone()).await;
        let dir = tempfile::tempdir().expect("tmp");
        let dest = dir.path().join("a.bin");
        let req = request(server.uri(), dest, body.len(), None);
        let calls = Mutex::new(Vec::new());

        download_one(
            &fetcher(),
            req,
            &FakeSleeper::new(),
            &CancellationToken::new(),
            &|done, total| calls.lock().expect("registro").push((done, total)),
        )
        .await
        .expect("descarga");

        let calls = calls.into_inner().expect("registro");
        assert_eq!(calls.last(), Some(&(body.len() as u64, body.len() as u64)));
    }
}
