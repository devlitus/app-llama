//! Comandos y eventos Tauri del gestor de descargas.
use std::error::Error;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use app_llama_core::download::{
    DownloadError, DownloadId, DownloadManager, DownloadProgress, DownloadSnapshot, EventSink,
    FailureKind, Fs4DiskSpace, HttpFetcher, JsonQueueStore, ManagerDeps, SystemClock, TokioSleeper,
    UnitRequest,
};
use app_llama_core::hf::{DownloadRequest, HfService};
use reqwest::Url;
use tauri::{AppHandle, Emitter, Runtime, State};

use super::error::ErrorPayload;
use super::hf::HF_BASE_URL;

const STATE_EVENT: &str = "downloads://state";
const PROGRESS_EVENT: &str = "downloads://progress";
const USER_AGENT: &str = concat!("app-llama/", env!("CARGO_PKG_VERSION"));
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
// Sin timeout total (las descargas son largas): este solo corta una conexión colgada.
const READ_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_ACTIVE: usize = 1;
const MODELS_DIR: &str = "models";

/// Emite los eventos a la UI. Son informativos: si `emit` falla, la descarga sigue.
pub struct TauriEventSink<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> TauriEventSink<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

impl<R: Runtime> EventSink for TauriEventSink<R> {
    fn state(&self, snapshot: &DownloadSnapshot) {
        let _ = self.app.emit(STATE_EVENT, snapshot);
    }

    fn progress(&self, progress: &DownloadProgress) {
        let _ = self.app.emit(PROGRESS_EVENT, progress);
    }
}

pub struct DownloadsAppState {
    manager: Arc<DownloadManager>,
}

/// Estado con las implementaciones reales. Debe crearse después del de HF, del
/// que toma el servicio (y con él el token en memoria).
pub fn production_state<R: Runtime>(
    data_dir: &Path,
    hf: Arc<HfService>,
    app: AppHandle<R>,
) -> Result<DownloadsAppState, Box<dyn Error>> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .build()?;
    let hf_base_url = Url::parse(HF_BASE_URL)?;
    let manager = DownloadManager::new(ManagerDeps {
        fetcher: Arc::new(HttpFetcher::new(client, Some(hf_base_url.clone()))),
        clock: Arc::new(SystemClock::new()),
        sleeper: Arc::new(TokioSleeper),
        disk: Arc::new(Fs4DiskSpace),
        tokens: hf,
        store: Arc::new(JsonQueueStore::new(data_dir)),
        sink: Arc::new(TauriEventSink::new(app)),
        models_dir: data_dir.join(MODELS_DIR),
        hf_base_url,
        max_active: MAX_ACTIVE,
    })?;
    Ok(DownloadsAppState { manager })
}

impl From<DownloadError> for ErrorPayload {
    fn from(error: DownloadError) -> Self {
        Self {
            code: download_error_code(&error),
            message: download_error_message(&error),
        }
    }
}

fn download_error_code(error: &DownloadError) -> &'static str {
    match error {
        DownloadError::Failed(kind) => failure_code(kind),
        DownloadError::Cancelled => "Cancelled",
        DownloadError::UnsafePath => "UnsafePath",
        DownloadError::IncompleteSplit => "IncompleteSplit",
        DownloadError::InvalidRequest(_) => "InvalidRequest",
        DownloadError::AlreadyQueued => "AlreadyQueued",
        DownloadError::DestinationExists => "DestinationExists",
        DownloadError::InvalidTransition => "InvalidTransition",
        DownloadError::UnknownDownload => "UnknownDownload",
        DownloadError::Io(_) => "Io",
    }
}

fn failure_code(kind: &FailureKind) -> &'static str {
    match kind {
        FailureKind::Network => "Network",
        FailureKind::Unauthorized => "Unauthorized",
        FailureKind::Gated => "Gated",
        FailureKind::NotFound => "NotFound",
        FailureKind::ChecksumMismatch => "ChecksumMismatch",
        FailureKind::SizeMismatch { .. } => "SizeMismatch",
        FailureKind::InsufficientSpace { .. } => "InsufficientSpace",
        FailureKind::UnexpectedContentRange => "UnexpectedContentRange",
        FailureKind::DestinationExists => "DestinationExists",
        FailureKind::Io { .. } => "Io",
    }
}

/// Nunca incluye el detalle de los errores de E/S: podría arrastrar rutas o datos del usuario.
fn download_error_message(error: &DownloadError) -> String {
    match error {
        DownloadError::Io(_) => "Error de acceso al disco".to_owned(),
        DownloadError::Failed(FailureKind::Io { .. }) => {
            "La descarga falló por un error de acceso al disco".to_owned()
        }
        other => other.to_string(),
    }
}

#[tauri::command]
pub async fn downloads_enqueue_hf(
    state: State<'_, DownloadsAppState>,
    request: DownloadRequest,
) -> Result<DownloadId, ErrorPayload> {
    let request = UnitRequest::try_from(request)?;
    Ok(state.manager.enqueue(request).await?)
}

#[tauri::command]
pub async fn downloads_list(
    state: State<'_, DownloadsAppState>,
) -> Result<Vec<DownloadSnapshot>, ErrorPayload> {
    Ok(state.manager.list().await)
}

#[tauri::command]
pub async fn downloads_pause(
    state: State<'_, DownloadsAppState>,
    id: DownloadId,
) -> Result<(), ErrorPayload> {
    Ok(state.manager.pause(id).await?)
}

#[tauri::command]
pub async fn downloads_resume(
    state: State<'_, DownloadsAppState>,
    id: DownloadId,
) -> Result<(), ErrorPayload> {
    Ok(state.manager.resume(id).await?)
}

#[tauri::command]
pub async fn downloads_cancel(
    state: State<'_, DownloadsAppState>,
    id: DownloadId,
) -> Result<(), ErrorPayload> {
    Ok(state.manager.cancel(id).await?)
}

#[tauri::command]
pub async fn downloads_retry(
    state: State<'_, DownloadsAppState>,
    id: DownloadId,
) -> Result<(), ErrorPayload> {
    Ok(state.manager.retry(id).await?)
}

/// La única orden sin evento: su `Ok` es la señal de que la unidad ya no está en la cola.
#[tauri::command]
pub async fn downloads_remove(
    state: State<'_, DownloadsAppState>,
    id: DownloadId,
) -> Result<(), ErrorPayload> {
    Ok(state.manager.remove(id).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_llama_core::download::{
        DownloadId, DownloadProgress, DownloadSnapshot, FailureKind, FileSnapshot, FileStatus,
        ProgressPhase, UnitState,
    };
    use app_llama_core::hf::DownloadRequest;
    use serde_json::json;

    fn payload(error: DownloadError) -> ErrorPayload {
        ErrorPayload::from(error)
    }

    #[test]
    fn error_payload_download_codes() {
        let cases = [
            (DownloadError::UnsafePath, "UnsafePath"),
            (DownloadError::IncompleteSplit, "IncompleteSplit"),
            (DownloadError::AlreadyQueued, "AlreadyQueued"),
            (DownloadError::DestinationExists, "DestinationExists"),
            (DownloadError::InvalidTransition, "InvalidTransition"),
            (DownloadError::UnknownDownload, "UnknownDownload"),
            (DownloadError::InvalidRequest("x"), "InvalidRequest"),
            (DownloadError::Cancelled, "Cancelled"),
            (DownloadError::Io(std::io::Error::other("x")), "Io"),
            (DownloadError::Failed(FailureKind::Network), "Network"),
            (
                DownloadError::Failed(FailureKind::Unauthorized),
                "Unauthorized",
            ),
            (DownloadError::Failed(FailureKind::Gated), "Gated"),
            (DownloadError::Failed(FailureKind::NotFound), "NotFound"),
            (
                DownloadError::Failed(FailureKind::ChecksumMismatch),
                "ChecksumMismatch",
            ),
            (
                DownloadError::Failed(FailureKind::SizeMismatch {
                    expected: 1,
                    actual: 2,
                }),
                "SizeMismatch",
            ),
            (
                DownloadError::Failed(FailureKind::InsufficientSpace {
                    needed: 2,
                    available: 1,
                }),
                "InsufficientSpace",
            ),
            (
                DownloadError::Failed(FailureKind::UnexpectedContentRange),
                "UnexpectedContentRange",
            ),
            (
                DownloadError::Failed(FailureKind::DestinationExists),
                "DestinationExists",
            ),
            (
                DownloadError::Failed(FailureKind::Io {
                    message: "x".to_owned(),
                }),
                "Io",
            ),
        ];
        for (error, code) in cases {
            let payload = payload(error);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }

    #[test]
    fn error_payload_download_never_contains_token() {
        let token = format!("hf_{}", "a1B2".repeat(9));
        let errors = [
            DownloadError::Io(std::io::Error::other(format!("fallo con {token}"))),
            DownloadError::Failed(FailureKind::Io {
                message: format!("fallo con {token}"),
            }),
        ];
        for error in errors {
            let json = serde_json::to_string(&payload(error)).expect("serializa");
            assert!(!json.contains(&token));
        }
    }

    fn file(path: &str, size: u64, status: FileStatus) -> FileSnapshot {
        FileSnapshot {
            relative_path: path.to_owned(),
            size,
            status,
        }
    }

    fn snapshot(state: UnitState) -> DownloadSnapshot {
        DownloadSnapshot {
            id: DownloadId(7),
            version: 3,
            repo_id: "ana/modelo-GGUF".to_owned(),
            revision: "main".to_owned(),
            display_name: "modelo".to_owned(),
            state,
            total_bytes: 60,
            files: vec![
                file("m-00001-of-00002.gguf", 40, FileStatus::Verified),
                file("m-00002-of-00002.gguf", 20, FileStatus::Pending),
            ],
        }
    }

    // La UI (`ui/src/lib/downloads/types.ts`) espeja estas formas: si cambian, hay que actualizarla.
    #[test]
    fn json_download_snapshot_para_la_ui() {
        let snapshot = snapshot(UnitState::Failed(FailureKind::InsufficientSpace {
            needed: 65_498_251_264,
            available: 42_949_672_960,
        }));
        assert_eq!(
            serde_json::to_value(&snapshot).expect("serializa"),
            json!({
                "id": 7,
                "version": 3,
                "repo_id": "ana/modelo-GGUF",
                "revision": "main",
                "display_name": "modelo",
                "state": {"Failed": {"InsufficientSpace": {
                    "needed": 65_498_251_264_u64,
                    "available": 42_949_672_960_u64
                }}},
                "total_bytes": 60,
                "files": [
                    {"relative_path": "m-00001-of-00002.gguf", "size": 40, "status": "Verified"},
                    {"relative_path": "m-00002-of-00002.gguf", "size": 20, "status": "Pending"}
                ]
            })
        );
    }

    #[test]
    fn json_download_snapshot_estado_simple() {
        let value = serde_json::to_value(snapshot(UnitState::Paused)).expect("serializa");
        assert_eq!(value["state"], json!("Paused"));
    }

    #[test]
    fn json_download_progress_para_la_ui() {
        let progress = DownloadProgress {
            id: DownloadId(7),
            downloaded_bytes: 10,
            total_bytes: 20,
            bytes_per_sec: 5,
            eta_secs: None,
            phase: ProgressPhase::Verify,
        };
        assert_eq!(
            serde_json::to_string(&progress).expect("serializa"),
            r#"{"id":7,"downloaded_bytes":10,"total_bytes":20,"bytes_per_sec":5,"eta_secs":null,"phase":"Verify"}"#
        );
    }

    #[test]
    fn json_download_request_desde_la_ui() {
        let request: DownloadRequest = serde_json::from_value(json!({
            "repo_id": "ana/modelo-GGUF",
            "revision": "9217f5db79a29953eb74d5343926648285ec7e67",
            "parts": [
                {"path": "m-00001-of-00002.gguf", "size": 40, "sha256": null},
                {"path": "m-00002-of-00002.gguf", "size": 20, "sha256": "abc"}
            ]
        }))
        .expect("deserializa");
        assert_eq!(request.repo_id, "ana/modelo-GGUF");
        assert_eq!(request.parts.len(), 2);
        assert_eq!(request.parts[1].sha256.as_deref(), Some("abc"));
    }
}
