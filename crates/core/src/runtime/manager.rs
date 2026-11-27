//! `RuntimeManager`: estado del runtime, hardware, actualizaciones y binario externo.
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::{Mutex, MutexGuard};

use crate::runtime::{
    ActiveRuntime, ArtifactFetcher, Backend, CommandRunner, HardwareInfo, Os, ReleaseSource,
    RuntimeError, RuntimeSource, RuntimeState, RuntimeStatus, StateStore, SystemProbe,
    available_backends, build_number, detect, is_compatible, latest_build_release, status_of,
    verify_server,
};

pub trait DirRemover: Send + Sync {
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
}

pub struct StdDirRemover;

impl DirRemover for StdDirRemover {
    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::remove_dir_all(path)
    }
}

pub struct RuntimeDeps {
    pub source: Arc<dyn ReleaseSource>,
    pub fetcher: Arc<dyn ArtifactFetcher>,
    pub runner: Arc<dyn CommandRunner>,
    pub probe: Arc<dyn SystemProbe>,
    pub remover: Arc<dyn DirRemover>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateInfo {
    pub installed: Option<u32>,
    pub latest: u32,
    pub latest_tag: String,
    pub available_backends: Vec<Backend>,
    pub update_available: bool,
}

pub struct RuntimeManager {
    deps: RuntimeDeps,
    /// Serializa todo ciclo leer-modificar-guardar del estado: `StateStore` no es seguro
    /// para escrituras concurrentes.
    state: Mutex<StateStore>,
    /// Una sola instalación a la vez.
    installing: Mutex<()>,
}

impl RuntimeManager {
    pub fn new(runtime_dir: impl Into<PathBuf>, deps: RuntimeDeps) -> Self {
        Self {
            deps,
            state: Mutex::new(StateStore::new(runtime_dir.into())),
            installing: Mutex::new(()),
        }
    }

    /// Reintenta los borrados pendientes y devuelve el estado del runtime activo.
    pub async fn status(&self) -> Result<RuntimeStatus, RuntimeError> {
        let state = self.retry_pending_cleanup().await?;
        Ok(status_of(&state, Path::exists))
    }

    /// Bloqueante (ejecuta `nvidia-smi`): la detección va a un hilo aparte.
    pub async fn hardware(&self) -> Result<HardwareInfo, RuntimeError> {
        let probe = Arc::clone(&self.deps.probe);
        tokio::task::spawn_blocking(move || detect(probe.as_ref()))
            .await
            .map_err(|error| RuntimeError::Io(io::Error::other(error)))
    }

    /// Punto de integración con el plan 008: solo hay ruta si el runtime está `Ready`.
    pub async fn active_server_path(&self) -> Result<Option<PathBuf>, RuntimeError> {
        Ok(match self.status().await? {
            RuntimeStatus::Ready(active) => Some(active.server_path),
            _ => None,
        })
    }

    pub async fn check_update(&self) -> Result<UpdateInfo, RuntimeError> {
        let releases = self.deps.source.list_releases().await?;
        let hardware = self.hardware().await?;
        let latest = latest_build_release(&releases, |release| is_compatible(release, &hardware))?;
        let latest_build =
            build_number(&latest.tag_name).ok_or(RuntimeError::NoCompatibleRelease)?;
        let installed = self
            .load_state()
            .await?
            .active
            .map(|active| active.version.build);
        Ok(UpdateInfo {
            installed,
            latest: latest_build,
            latest_tag: latest.tag_name.clone(),
            available_backends: available_backends(latest, &hardware),
            update_available: installed.is_none_or(|installed| latest_build > installed),
        })
    }

    /// Activa un `llama-server` que el usuario ya tiene. No se modifica ni se borra nada.
    pub async fn use_external(&self, path: &Path) -> Result<ActiveRuntime, RuntimeError> {
        let _installing = self.begin_install()?;
        ensure_server_name(self.deps.probe.os(), path)?;
        // `verify_server` usa el directorio padre como cwd: necesita una ruta absoluta.
        let server_path = std::path::absolute(path)?;
        let version = verify_server(self.deps.runner.as_ref(), &server_path).await?;
        let active = ActiveRuntime {
            source: RuntimeSource::External,
            backend: None,
            server_path,
            version,
        };
        self.modify_state(|state| state.active = Some(active.clone()))
            .await?;
        Ok(active)
    }

    /// Reserva la instalación; mientras se conserve la guarda, otra llamada falla con
    /// `AlreadyInstalling`.
    pub fn begin_install(&self) -> Result<MutexGuard<'_, ()>, RuntimeError> {
        self.installing
            .try_lock()
            .map_err(|_| RuntimeError::AlreadyInstalling)
    }

    async fn load_state(&self) -> Result<RuntimeState, RuntimeError> {
        self.state.lock().await.load()
    }

    /// Ciclo leer-modificar-guardar bajo el cerrojo del estado; solo escribe si hubo cambios.
    pub(super) async fn modify_state(
        &self,
        change: impl FnOnce(&mut RuntimeState),
    ) -> Result<RuntimeState, RuntimeError> {
        let store = self.state.lock().await;
        let original = store.load()?;
        let mut state = original.clone();
        change(&mut state);
        if state != original {
            store.save(&state)?;
        }
        Ok(state)
    }

    async fn retry_pending_cleanup(&self) -> Result<RuntimeState, RuntimeError> {
        let store = self.state.lock().await;
        let mut state = store.load()?;
        if state.pending_cleanup.is_empty() {
            return Ok(state);
        }
        let pending = std::mem::take(&mut state.pending_cleanup);
        let pending_before = pending.len();
        state.pending_cleanup = remove_dirs(Arc::clone(&self.deps.remover), pending).await?;
        if state.pending_cleanup.len() != pending_before {
            store.save(&state)?;
        }
        Ok(state)
    }
}

/// Borra los directorios en un hilo bloqueante y devuelve los que no se pudieron borrar.
/// Un directorio que ya no existe cuenta como borrado.
pub(super) async fn remove_dirs(
    remover: Arc<dyn DirRemover>,
    paths: Vec<PathBuf>,
) -> Result<Vec<PathBuf>, RuntimeError> {
    tokio::task::spawn_blocking(move || {
        paths
            .into_iter()
            .filter(|path| match remover.remove_dir_all(path) {
                Ok(()) => false,
                Err(error) => error.kind() != io::ErrorKind::NotFound,
            })
            .collect()
    })
    .await
    .map_err(|error| RuntimeError::Io(io::Error::other(error)))
}

fn ensure_server_name(os: Os, path: &Path) -> Result<(), RuntimeError> {
    let is_server = path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| is_server_name(os, name));
    if is_server {
        Ok(())
    } else {
        Err(RuntimeError::InvalidExternalBinary)
    }
}

/// Windows ignora mayúsculas/minúsculas en los nombres de fichero; macOS (por defecto) no.
fn is_server_name(os: Os, name: &str) -> bool {
    match os {
        Os::Windows => name.eq_ignore_ascii_case("llama-server.exe"),
        Os::MacOs => name == "llama-server",
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use super::*;
    use crate::runtime::test_support::{
        FakeFetcher, FakeProbe, FakeRemover, FakeRunner, FakeSource,
    };
    use crate::runtime::{Arch, CpuIsa, RuntimeVersion, parse_releases};

    const RELEASES: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/releases_b12780.json"
    ));
    const SMI_HEADER: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/nvidia_smi_header.txt"
    ));
    const GPU_ARGS: [&str; 2] = [
        "--query-gpu=name,memory.total",
        "--format=csv,noheader,nounits",
    ];

    fn windows_probe() -> FakeProbe {
        FakeProbe::new(Os::Windows, Arch::X86_64).with_cpu(&["avx2"])
    }

    fn cuda_vulkan_probe() -> FakeProbe {
        let root = PathBuf::from("sysroot");
        let vulkan_dll = root.join("System32").join("vulkan-1.dll");
        windows_probe()
            .with_output("nvidia-smi", &GPU_ARGS, "NVIDIA GeForce RTX 4070, 12282\n")
            .with_output("nvidia-smi", &[], SMI_HEADER)
            .with_system_root(root)
            .with_file(vulkan_dll)
    }

    struct Fixture {
        _temp: tempfile::TempDir,
        runtime_dir: PathBuf,
        runner: Arc<FakeRunner>,
        manager: RuntimeManager,
    }

    fn fixture(
        source: FakeSource,
        probe: FakeProbe,
        runner: FakeRunner,
        remover: FakeRemover,
    ) -> Fixture {
        let temp = tempfile::tempdir().unwrap();
        let runtime_dir = temp.path().join("runtime");
        let runner = Arc::new(runner);
        let manager = RuntimeManager::new(
            &runtime_dir,
            RuntimeDeps {
                source: Arc::new(source),
                fetcher: Arc::new(FakeFetcher),
                runner: runner.clone(),
                probe: Arc::new(probe),
                remover: Arc::new(remover),
            },
        );
        Fixture {
            _temp: temp,
            runtime_dir,
            runner,
            manager,
        }
    }

    fn default_fixture() -> Fixture {
        fixture(
            FakeSource::with_releases(parse_releases(RELEASES).unwrap()),
            windows_probe(),
            FakeRunner::exits(0, "", "version: 12000 (abc1234)"),
            FakeRemover::deleting(),
        )
    }

    impl Fixture {
        fn store(&self) -> StateStore {
            StateStore::new(&self.runtime_dir)
        }

        fn save(&self, state: &RuntimeState) {
            self.store().save(state).unwrap();
        }

        fn saved(&self) -> RuntimeState {
            self.store().load().unwrap()
        }

        fn external_binary(&self, name: &str) -> PathBuf {
            let path = self.runtime_dir.join(name);
            std::fs::create_dir_all(&self.runtime_dir).unwrap();
            std::fs::write(&path, "").unwrap();
            path
        }
    }

    fn managed(build: u32) -> ActiveRuntime {
        let tag = format!("b{build}");
        ActiveRuntime {
            source: RuntimeSource::Managed { tag: tag.clone() },
            backend: Some(Backend::Cpu {
                isa: CpuIsa::Generic,
            }),
            server_path: PathBuf::from("runtime").join(tag).join("llama-server.exe"),
            version: RuntimeVersion {
                build,
                commit: "abc1234".to_string(),
            },
        }
    }

    fn state_with(active: Option<ActiveRuntime>) -> RuntimeState {
        RuntimeState {
            active,
            ..RuntimeState::default()
        }
    }

    #[tokio::test]
    async fn status_sin_estado() {
        let fx = default_fixture();
        assert_eq!(
            fx.manager.status().await.unwrap(),
            RuntimeStatus::NotInstalled
        );
        assert_eq!(fx.manager.active_server_path().await.unwrap(), None);
    }

    #[tokio::test]
    async fn status_error_de_lectura_se_propaga() {
        let fx = default_fixture();
        std::fs::create_dir_all(fx.runtime_dir.join("runtime.json")).unwrap();
        assert!(matches!(
            fx.manager.status().await,
            Err(RuntimeError::Io(_))
        ));
    }

    #[tokio::test]
    async fn status_reintenta_pendientes() {
        let fx = default_fixture();
        let pending = fx.runtime_dir.join("b12779");
        std::fs::create_dir_all(&pending).unwrap();
        fx.save(&RuntimeState {
            pending_cleanup: vec![pending.clone()],
            ..RuntimeState::default()
        });
        fx.manager.status().await.unwrap();
        assert!(!pending.exists());
        assert_eq!(fx.saved().pending_cleanup, Vec::<PathBuf>::new());
    }

    #[tokio::test]
    async fn status_pendiente_sigue_fallando() {
        let fx = fixture(
            FakeSource::with_releases(Vec::new()),
            windows_probe(),
            FakeRunner::exits(0, "", ""),
            FakeRemover::failing(),
        );
        let pending = fx.runtime_dir.join("b12779");
        fx.save(&RuntimeState {
            pending_cleanup: vec![pending.clone()],
            ..RuntimeState::default()
        });
        assert!(fx.manager.status().await.is_ok());
        assert_eq!(fx.saved().pending_cleanup, vec![pending]);
    }

    #[tokio::test]
    async fn status_pendiente_inexistente_se_quita() {
        let fx = default_fixture();
        let missing = fx.runtime_dir.join("b12779");
        fx.save(&RuntimeState {
            pending_cleanup: vec![missing],
            ..RuntimeState::default()
        });
        fx.manager.status().await.unwrap();
        assert_eq!(fx.saved().pending_cleanup, Vec::<PathBuf>::new());
    }

    #[tokio::test]
    async fn remove_dirs_devuelve_solo_los_que_fallan() {
        let temp = tempfile::tempdir().unwrap();
        let existing = temp.path().join("a");
        std::fs::create_dir(&existing).unwrap();
        let missing = temp.path().join("b");
        let remover: Arc<dyn DirRemover> = Arc::new(FakeRemover::deleting());
        let left = remove_dirs(Arc::clone(&remover), vec![existing.clone(), missing])
            .await
            .unwrap();
        assert!(left.is_empty());
        assert!(!existing.exists());
        let failing: Arc<dyn DirRemover> = Arc::new(FakeRemover::failing());
        let path = temp.path().join("c");
        let left = remove_dirs(failing, vec![path.clone()]).await.unwrap();
        assert_eq!(left, vec![path]);
    }

    #[test]
    fn nombre_de_servidor_por_plataforma() {
        assert!(is_server_name(Os::Windows, "llama-server.exe"));
        assert!(is_server_name(Os::Windows, "LLAMA-SERVER.EXE"));
        assert!(is_server_name(Os::Windows, "Llama-Server.exe"));
        assert!(!is_server_name(Os::Windows, "llama-cli.exe"));
        assert!(!is_server_name(Os::Windows, "llama-server"));
        assert!(is_server_name(Os::MacOs, "llama-server"));
        assert!(!is_server_name(Os::MacOs, "LLAMA-SERVER"));
        assert!(!is_server_name(Os::MacOs, "llama-server.exe"));
    }

    #[tokio::test]
    async fn external_durante_instalacion_rechazado() {
        let fx = default_fixture();
        let binary = fx.external_binary("llama-server.exe");
        let _install = fx.manager.begin_install().unwrap();
        let result = fx.manager.use_external(&binary).await;
        assert!(matches!(result, Err(RuntimeError::AlreadyInstalling)));
        assert_eq!(fx.saved(), RuntimeState::default());
        assert!(fx.runner.calls().is_empty());
    }

    #[tokio::test]
    async fn hardware_usa_la_sonda() {
        let hw = default_fixture().manager.hardware().await.unwrap();
        assert_eq!((hw.os, hw.arch, hw.avx2), (Os::Windows, Arch::X86_64, true));
    }

    #[tokio::test]
    async fn check_update_disponible() {
        let fx = default_fixture();
        fx.save(&state_with(Some(managed(12779))));
        let info = fx.manager.check_update().await.unwrap();
        assert_eq!(info.installed, Some(12779));
        assert_eq!(info.latest, 12780);
        assert_eq!(info.latest_tag, "b12780");
        assert!(info.update_available);
    }

    #[tokio::test]
    async fn check_update_al_dia() {
        let fx = default_fixture();
        fx.save(&state_with(Some(managed(12780))));
        let info = fx.manager.check_update().await.unwrap();
        assert!(!info.update_available);
    }

    #[tokio::test]
    async fn check_update_sin_instalar() {
        let info = default_fixture().manager.check_update().await.unwrap();
        assert_eq!(info.installed, None);
        assert!(info.update_available);
    }

    #[tokio::test]
    async fn check_update_backends() {
        let fx = fixture(
            FakeSource::with_releases(parse_releases(RELEASES).unwrap()),
            cuda_vulkan_probe(),
            FakeRunner::exits(0, "", ""),
            FakeRemover::deleting(),
        );
        let info = fx.manager.check_update().await.unwrap();
        assert_eq!(
            info.available_backends,
            vec![
                Backend::Cuda {
                    major: 13,
                    minor: 5
                },
                Backend::Vulkan,
                Backend::Cpu {
                    isa: CpuIsa::Generic
                }
            ]
        );
    }

    #[tokio::test]
    async fn check_update_rate_limit() {
        let fx = fixture(
            FakeSource::rate_limited(),
            windows_probe(),
            FakeRunner::exits(0, "", ""),
            FakeRemover::deleting(),
        );
        let state = state_with(Some(managed(12779)));
        fx.save(&state);
        assert!(matches!(
            fx.manager.check_update().await,
            Err(RuntimeError::RateLimited { .. })
        ));
        assert_eq!(fx.saved(), state);
    }

    #[tokio::test]
    async fn external_valido() {
        let fx = default_fixture();
        let binary = fx.external_binary("llama-server.exe");
        let active = fx.manager.use_external(&binary).await.unwrap();
        assert_eq!(active.source, RuntimeSource::External);
        assert_eq!(active.backend, None);
        assert_eq!(active.version.build, 12000);
        assert_eq!(
            fx.manager.status().await.unwrap(),
            RuntimeStatus::Ready(active)
        );
        assert_eq!(
            fx.manager.active_server_path().await.unwrap(),
            Some(binary.clone())
        );
        let calls = fx.runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].program, binary);
        assert_eq!(calls[0].args, ["--version"]);
    }

    #[tokio::test]
    async fn external_ruta_relativa_se_guarda_absoluta() {
        let fx = default_fixture();
        let active = fx
            .manager
            .use_external(Path::new("llama-server.exe"))
            .await
            .unwrap();
        let expected = std::path::absolute("llama-server.exe").unwrap();
        assert_eq!(active.server_path, expected);
        assert_eq!(fx.runner.calls()[0].program, expected);
        assert_eq!(fx.saved().active, Some(active));
    }

    #[tokio::test]
    async fn external_nombre_invalido() {
        let fx = default_fixture();
        let result = fx.manager.use_external(Path::new("llama-cli.exe")).await;
        assert!(matches!(result, Err(RuntimeError::InvalidExternalBinary)));
        assert_eq!(fx.saved(), RuntimeState::default());
        assert!(fx.runner.calls().is_empty());
    }

    #[tokio::test]
    async fn external_verificacion_falla() {
        let fx = fixture(
            FakeSource::with_releases(Vec::new()),
            windows_probe(),
            FakeRunner::exits(1, "", "boom"),
            FakeRemover::deleting(),
        );
        let state = state_with(Some(managed(12779)));
        fx.save(&state);
        let binary = fx.external_binary("llama-server.exe");
        let result = fx.manager.use_external(&binary).await;
        assert!(matches!(
            result,
            Err(RuntimeError::VerificationFailed { .. })
        ));
        assert_eq!(fx.saved(), state);
    }

    #[tokio::test]
    async fn external_no_borra_gestionado() {
        let fx = default_fixture();
        let managed_dir = fx.runtime_dir.join("b12780");
        std::fs::create_dir_all(&managed_dir).unwrap();
        fx.save(&state_with(Some(managed(12780))));
        let binary = fx.external_binary("llama-server.exe");
        fx.manager.use_external(&binary).await.unwrap();
        assert!(managed_dir.is_dir());
    }

    #[tokio::test]
    async fn instalacion_concurrente_rechazada() {
        let fx = default_fixture();
        let _first = fx.manager.begin_install().unwrap();
        assert!(matches!(
            fx.manager.begin_install(),
            Err(RuntimeError::AlreadyInstalling)
        ));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn escrituras_concurrentes_del_estado_no_se_pisan() {
        let fx = default_fixture();
        let binary = fx.external_binary("llama-server.exe");
        let pending = fx.runtime_dir.join("b12779");
        std::fs::create_dir_all(&pending).unwrap();
        fx.save(&RuntimeState {
            pending_cleanup: vec![pending],
            ..RuntimeState::default()
        });
        let runtime_dir = fx.runtime_dir.clone();
        let manager = Arc::new(fx.manager);
        let external = {
            let manager = Arc::clone(&manager);
            tokio::spawn(async move { manager.use_external(&binary).await.map(|_| ()) })
        };
        let statuses: Vec<_> = (0..7)
            .map(|_| {
                let manager = Arc::clone(&manager);
                tokio::spawn(async move { manager.status().await.map(|_| ()) })
            })
            .collect();
        assert!(external.await.unwrap().is_ok());
        for task in statuses {
            assert!(task.await.unwrap().is_ok());
        }
        let state = StateStore::new(&runtime_dir).load().unwrap();
        assert_eq!(
            state.active.map(|a| a.source),
            Some(RuntimeSource::External)
        );
        assert!(state.pending_cleanup.is_empty());
    }
}
