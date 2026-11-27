//! Comandos y eventos Tauri del runtime de llama.cpp.
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use app_llama_core::runtime::{
    ActiveRuntime, BackendPreference, CancelToken, GitHubReleaseSource, HardwareInfo, NativeProbe,
    RuntimeDeps, RuntimeError, RuntimeManager, RuntimeStatus, SimpleHttpFetcher, StdDirRemover,
    TokioCommandRunner, UpdateInfo,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime, State};

const PROGRESS_EVENT: &str = "runtime://progress";
const GITHUB_API: &str = "https://api.github.com";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
// Sin timeout total (las descargas son largas): este solo corta una conexión colgada, que si
// no impediría cancelar.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Serialize)]
pub struct ErrorPayload {
    pub code: &'static str,
    pub message: String,
}

fn error_code(error: &RuntimeError) -> &'static str {
    match error {
        RuntimeError::ReleaseParse(_) => "ReleaseParse",
        RuntimeError::NoCompatibleRelease => "NoCompatibleRelease",
        RuntimeError::NoCompatibleAsset => "NoCompatibleAsset",
        RuntimeError::BackendUnavailable => "BackendUnavailable",
        RuntimeError::Http(_) => "Http",
        RuntimeError::Io(_) => "Io",
        RuntimeError::HttpStatus(_) => "HttpStatus",
        RuntimeError::RateLimited { .. } => "RateLimited",
        RuntimeError::SizeMismatch { .. } => "SizeMismatch",
        RuntimeError::IntegrityMismatch => "IntegrityMismatch",
        RuntimeError::Cancelled => "Cancelled",
        RuntimeError::UnsafeArchivePath(_) => "UnsafeArchivePath",
        RuntimeError::Extract(_) => "Extract",
        RuntimeError::ServerBinaryNotFound => "ServerBinaryNotFound",
        RuntimeError::VerificationFailed { .. } => "VerificationFailed",
        RuntimeError::QuarantineFailed(_) => "QuarantineFailed",
        RuntimeError::AlreadyInstalling => "AlreadyInstalling",
        RuntimeError::InvalidExternalBinary => "InvalidExternalBinary",
    }
}

/// El mensaje del error más, si la hay, la causa subyacente (que `Display` no repite).
fn error_message(error: &RuntimeError) -> String {
    if matches!(error, RuntimeError::RateLimited { .. }) {
        return "Se alcanzó el límite de GitHub para consultar las versiones. \
                Inténtalo más tarde."
            .to_string();
    }
    let mut message = error.to_string();
    let mut cause = error.source();
    while let Some(inner) = cause {
        message.push_str(": ");
        message.push_str(&inner.to_string());
        cause = inner.source();
    }
    message
}

pub fn error_to_payload(error: &RuntimeError) -> ErrorPayload {
    ErrorPayload {
        code: error_code(error),
        message: error_message(error),
    }
}

impl From<RuntimeError> for ErrorPayload {
    fn from(error: RuntimeError) -> Self {
        error_to_payload(&error)
    }
}

pub struct RuntimeAppState {
    manager: Arc<RuntimeManager>,
    cancel: Mutex<Option<CancelToken>>,
}

impl RuntimeAppState {
    fn new(manager: RuntimeManager) -> Self {
        Self {
            manager: Arc::new(manager),
            cancel: Mutex::new(None),
        }
    }

    /// Registra el token de la instalación que empieza; `None` si ya hay una en curso, para no
    /// pisar el token de la que se puede cancelar.
    fn begin_install(&self) -> Option<CancelToken> {
        let mut slot = self.cancel.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.is_some() {
            return None;
        }
        let token = CancelToken::default();
        *slot = Some(token.clone());
        Some(token)
    }

    fn finish_install(&self) {
        *self.cancel.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// `true` si había una instalación en curso a la que pedir la cancelación.
    fn cancel_install(&self) -> bool {
        let slot = self.cancel.lock().unwrap_or_else(PoisonError::into_inner);
        slot.as_ref().map(CancelToken::cancel).is_some()
    }
}

/// Estado con las implementaciones reales. No toca el disco: `runtime_dir` solo se crea al
/// instalar.
pub fn production_state(app_data_dir: &Path) -> Result<RuntimeAppState, reqwest::Error> {
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(READ_TIMEOUT)
        .build()?;
    let deps = RuntimeDeps {
        source: Arc::new(GitHubReleaseSource::new(client.clone(), GITHUB_API)),
        fetcher: Arc::new(SimpleHttpFetcher::new(client)),
        runner: Arc::new(TokioCommandRunner),
        probe: Arc::new(NativeProbe),
        remover: Arc::new(StdDirRemover),
    };
    Ok(RuntimeAppState::new(RuntimeManager::new(
        app_data_dir.join("runtime"),
        deps,
    )))
}

#[tauri::command]
pub async fn runtime_hardware(
    state: State<'_, RuntimeAppState>,
) -> Result<HardwareInfo, ErrorPayload> {
    Ok(state.manager.hardware().await?)
}

#[tauri::command]
pub async fn runtime_status(
    state: State<'_, RuntimeAppState>,
) -> Result<RuntimeStatus, ErrorPayload> {
    Ok(state.manager.status().await?)
}

#[tauri::command]
pub async fn runtime_check_update(
    state: State<'_, RuntimeAppState>,
) -> Result<UpdateInfo, ErrorPayload> {
    Ok(state.manager.check_update().await?)
}

#[tauri::command]
pub async fn runtime_install<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, RuntimeAppState>,
    preference: BackendPreference,
) -> Result<ActiveRuntime, ErrorPayload> {
    let cancel = state
        .begin_install()
        .ok_or(RuntimeError::AlreadyInstalling)?;
    // El progreso es informativo: si no se puede emitir, la instalación sigue.
    let on_progress = |event| {
        let _ = app.emit(PROGRESS_EVENT, event);
    };
    let result = state
        .manager
        .install(preference, &on_progress, &cancel)
        .await;
    state.finish_install();
    Ok(result?)
}

#[tauri::command]
pub fn runtime_cancel_install(state: State<'_, RuntimeAppState>) {
    state.cancel_install();
}

#[tauri::command]
pub async fn runtime_use_external(
    state: State<'_, RuntimeAppState>,
    path: PathBuf,
) -> Result<ActiveRuntime, ErrorPayload> {
    Ok(state.manager.use_external(&path).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_llama_core::runtime::{ProgressEvent, ProgressPhase};

    fn payload(error: RuntimeError) -> ErrorPayload {
        error_to_payload(&error)
    }

    #[test]
    fn error_payload_codigos() {
        let cases = [
            (
                RuntimeError::RateLimited { reset_epoch: None },
                "RateLimited",
            ),
            (
                RuntimeError::VerificationFailed {
                    stderr_tail: String::new(),
                },
                "VerificationFailed",
            ),
            (RuntimeError::Cancelled, "Cancelled"),
            (RuntimeError::NoCompatibleAsset, "NoCompatibleAsset"),
            (RuntimeError::AlreadyInstalling, "AlreadyInstalling"),
            (RuntimeError::Io(std::io::Error::other("x")), "Io"),
        ];
        for (error, code) in cases {
            let payload = payload(error);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }

    #[test]
    fn error_payload_rate_limit_mensaje() {
        let message = payload(RuntimeError::RateLimited {
            reset_epoch: Some(1_800_000_000),
        })
        .message;
        assert!(message.contains("límite de GitHub"));
        assert!(message.contains("Inténtalo más tarde"));
    }

    #[test]
    fn error_payload_verificacion_incluye_stderr() {
        let message = payload(RuntimeError::VerificationFailed {
            stderr_tail: "dyld: Library not loaded".to_string(),
        })
        .message;
        assert!(message.contains("dyld: Library not loaded"));
    }

    #[test]
    fn error_payload_io_incluye_la_causa() {
        let message = payload(RuntimeError::Io(std::io::Error::other("disco lleno"))).message;
        assert!(message.contains("disco lleno"));
    }

    #[test]
    fn error_payload_serializa_code_y_message() {
        let value = serde_json::to_value(payload(RuntimeError::Cancelled)).expect("serializa");
        assert_eq!(value["code"], "Cancelled");
        assert!(value["message"].is_string());
    }

    #[test]
    fn progress_event_serializa_camel_case() {
        let event = ProgressEvent {
            phase: ProgressPhase::Fetching { artifact: "main" },
            done: 1,
            total: 2,
        };
        assert_eq!(
            serde_json::to_string(&event).expect("serializa"),
            r#"{"phase":{"kind":"fetching","artifact":"main"},"done":1,"total":2}"#
        );
    }

    #[test]
    fn preference_deserializa() {
        let parse = |text: &str| serde_json::from_str::<BackendPreference>(text);
        assert_eq!(parse("\"auto\"").expect("auto"), BackendPreference::Auto);
        assert_eq!(parse("\"cuda\"").expect("cuda"), BackendPreference::Cuda);
        assert!(parse("\"x\"").is_err());
    }

    #[test]
    fn cancel_sin_instalacion_no_hace_nada() {
        let state = test_state();
        assert!(!state.cancel_install());
    }

    #[test]
    fn cancel_marca_el_token_de_la_instalacion_en_curso() {
        let state = test_state();
        let token = state.begin_install().expect("primera instalación");
        assert!(state.cancel_install());
        assert!(token.is_cancelled());
    }

    #[test]
    fn segunda_instalacion_no_pisa_el_token() {
        let state = test_state();
        let first = state.begin_install().expect("primera instalación");
        assert!(state.begin_install().is_none());
        assert!(state.cancel_install());
        assert!(first.is_cancelled());
    }

    #[test]
    fn terminar_libera_el_token() {
        let state = test_state();
        state.begin_install().expect("primera instalación");
        state.finish_install();
        assert!(!state.cancel_install());
        assert!(state.begin_install().is_some());
    }

    fn test_state() -> RuntimeAppState {
        let temp = tempfile::tempdir().expect("tempdir");
        production_state(temp.path()).expect("el cliente HTTP se construye")
    }
}
