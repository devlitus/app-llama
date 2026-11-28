//! Comandos y eventos Tauri del runtime de llama.cpp.
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use app_llama_core::download::TokioSleeper;
use app_llama_core::runtime::{
    ActiveRuntime, BackendPreference, CancelToken, GitHubReleaseSource, HardwareInfo, NativeProbe,
    ResumableHttpFetcher, RuntimeDeps, RuntimeError, RuntimeManager, RuntimeStatus, StdDirRemover,
    TokioCommandRunner, UpdateInfo,
};
use tauri::{AppHandle, Emitter, Runtime, State};

use super::error::ErrorPayload;

const PROGRESS_EVENT: &str = "runtime://progress";
const GITHUB_API: &str = "https://api.github.com";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
// Sin timeout total (las descargas son largas): este solo corta una conexión colgada, que si
// no impediría cancelar.
const READ_TIMEOUT: Duration = Duration::from_secs(60);

fn error_code(error: &RuntimeError) -> &'static str {
    match error {
        RuntimeError::ReleaseParse(_) => "ReleaseParse",
        RuntimeError::NoCompatibleRelease => "NoCompatibleRelease",
        RuntimeError::NoCompatibleAsset => "NoCompatibleAsset",
        RuntimeError::BackendUnavailable => "BackendUnavailable",
        RuntimeError::Http(_) => "Http",
        RuntimeError::Network => "Network",
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

    pub async fn hardware(&self) -> Result<HardwareInfo, RuntimeError> {
        self.manager.hardware().await
    }

    /// Reserva la instalación que empieza; `None` si ya hay una en curso, para no pisar el
    /// token de la que se puede cancelar. La guarda libera la reserva al soltarse, también si
    /// la instalación entra en pánico o se descarta su future.
    fn begin_install(&self) -> Option<InstallSlot<'_>> {
        let mut slot = self.cancel.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.is_some() {
            return None;
        }
        let token = CancelToken::default();
        *slot = Some(token.clone());
        Some(InstallSlot { state: self, token })
    }

    /// `true` si había una instalación en curso a la que pedir la cancelación.
    fn cancel_install(&self) -> bool {
        let slot = self.cancel.lock().unwrap_or_else(PoisonError::into_inner);
        slot.as_ref().map(CancelToken::cancel).is_some()
    }
}

struct InstallSlot<'a> {
    state: &'a RuntimeAppState,
    token: CancelToken,
}

impl InstallSlot<'_> {
    fn token(&self) -> &CancelToken {
        &self.token
    }
}

impl Drop for InstallSlot<'_> {
    fn drop(&mut self) {
        *self
            .state
            .cancel
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
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
        fetcher: Arc::new(ResumableHttpFetcher::new(client, Arc::new(TokioSleeper))),
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
    Ok(state.hardware().await?)
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
    let slot = state
        .begin_install()
        .ok_or(RuntimeError::AlreadyInstalling)?;
    // El progreso es informativo: si no se puede emitir, la instalación sigue.
    let on_progress = |event| {
        let _ = app.emit(PROGRESS_EVENT, event);
    };
    Ok(state
        .manager
        .install(preference, &on_progress, slot.token())
        .await?)
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
            (RuntimeError::Network, "Network"),
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
        let slot = state.begin_install().expect("primera instalación");
        assert!(state.cancel_install());
        assert!(slot.token().is_cancelled());
    }

    #[test]
    fn segunda_instalacion_no_pisa_el_token() {
        let state = test_state();
        let first = state.begin_install().expect("primera instalación");
        assert!(state.begin_install().is_none());
        assert!(state.cancel_install());
        assert!(first.token().is_cancelled());
    }

    #[test]
    fn soltar_la_guarda_libera_el_token() {
        let state = test_state();
        drop(state.begin_install().expect("primera instalación"));
        assert!(!state.cancel_install());
        assert!(state.begin_install().is_some());
    }

    #[test]
    fn la_guarda_libera_el_token_tras_un_error() {
        let state = test_state();
        let attempt = |state: &RuntimeAppState| -> Result<(), RuntimeError> {
            let _slot = state
                .begin_install()
                .ok_or(RuntimeError::AlreadyInstalling)?;
            Err(RuntimeError::Cancelled)
        };
        assert!(matches!(attempt(&state), Err(RuntimeError::Cancelled)));
        assert!(matches!(attempt(&state), Err(RuntimeError::Cancelled)));
    }

    #[test]
    fn la_guarda_libera_el_token_si_hay_panico() {
        let state = test_state();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _slot = state.begin_install().expect("primera instalación");
            panic!("fallo en la instalación");
        }));
        assert!(result.is_err());
        assert!(state.begin_install().is_some());
    }

    // La UI (`ui/src/lib/runtime/types.ts`) espeja estas formas: si cambian, hay que actualizarla.
    #[test]
    fn json_hardware_status_y_update_para_la_ui() {
        use app_llama_core::runtime::{
            Arch, Backend, CpuIsa, CudaVersion, GpuInfo, Os, RuntimeSource, RuntimeVersion,
        };
        let hardware = HardwareInfo {
            os: Os::MacOs,
            arch: Arch::Aarch64,
            avx2: true,
            avx512: false,
            gpus: vec![GpuInfo {
                name: "RTX".to_string(),
                vram_mib: Some(12282),
            }],
            driver_cuda: Some(CudaVersion {
                major: 13,
                minor: 5,
            }),
            vulkan_available: true,
            metal: false,
            ram_bytes: Some(34_359_738_368),
        };
        assert_eq!(
            serde_json::to_string(&hardware).expect("serializa"),
            concat!(
                r#"{"os":"MacOs","arch":"Aarch64","avx2":true,"avx512":false,"#,
                r#""gpus":[{"name":"RTX","vram_mib":12282}],"#,
                r#""driver_cuda":{"major":13,"minor":5},"vulkan_available":true,"metal":false,"#,
                r#""ram_bytes":34359738368}"#
            )
        );

        let active = |source, backend| ActiveRuntime {
            source,
            backend,
            server_path: PathBuf::from("srv"),
            version: RuntimeVersion {
                build: 12780,
                commit: "abc".to_string(),
            },
        };
        let managed = RuntimeStatus::Ready(active(
            RuntimeSource::Managed {
                tag: "b12780".to_string(),
            },
            Some(Backend::Cpu { isa: CpuIsa::Avx2 }),
        ));
        assert_eq!(
            serde_json::to_string(&managed).expect("serializa"),
            concat!(
                r#"{"Ready":{"source":{"Managed":{"tag":"b12780"}},"#,
                r#""backend":{"Cpu":{"isa":"Avx2"}},"server_path":"srv","#,
                r#""version":{"build":12780,"commit":"abc"}}}"#
            )
        );
        let external = RuntimeStatus::Ready(active(RuntimeSource::External, None));
        assert_eq!(
            serde_json::to_string(&external).expect("serializa"),
            concat!(
                r#"{"Ready":{"source":"External","backend":null,"server_path":"srv","#,
                r#""version":{"build":12780,"commit":"abc"}}}"#
            )
        );
        assert_eq!(
            serde_json::to_string(&RuntimeStatus::NotInstalled).expect("serializa"),
            r#""NotInstalled""#
        );
        assert_eq!(
            serde_json::to_string(&RuntimeStatus::Broken {
                path: PathBuf::from("srv")
            })
            .expect("serializa"),
            r#"{"Broken":{"path":"srv"}}"#
        );

        let update = UpdateInfo {
            installed: None,
            latest: 12780,
            latest_tag: "b12780".to_string(),
            available_backends: vec![
                Backend::Cuda {
                    major: 13,
                    minor: 5,
                },
                Backend::Vulkan,
                Backend::Metal,
            ],
            update_available: true,
        };
        assert_eq!(
            serde_json::to_string(&update).expect("serializa"),
            concat!(
                r#"{"installed":null,"latest":12780,"latest_tag":"b12780","#,
                r#""available_backends":[{"Cuda":{"major":13,"minor":5}},"Vulkan","Metal"],"#,
                r#""update_available":true}"#
            )
        );
    }

    fn test_state() -> RuntimeAppState {
        let temp = tempfile::tempdir().expect("tempdir");
        production_state(temp.path()).expect("el cliente HTTP se construye")
    }
}
