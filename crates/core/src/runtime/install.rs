//! `RuntimeManager::install`: instalación gestionada de llama.cpp, actualización y limpieza.
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;

use crate::runtime::manager::{join_failure, remove_dirs};
use crate::runtime::{
    ActiveRuntime, ArchiveFormat, Asset, Backend, BackendPreference, CancelToken, CpuIsa,
    FetchRequest, InstallPlan, Os, RuntimeError, RuntimeManager, RuntimeSource, build_number,
    classify_asset, extract_archive, find_server_binary, is_compatible, latest_build_release,
    remove_quarantine, select_install, verify_server,
};

const STAGING_PREFIX: &str = ".staging-";

/// Directorio gestionado de una build: `<tag>-<backend>`, p. ej. `b12780-cuda-13.5`. Incluir el
/// backend evita renombrar sobre el directorio activo al reinstalar el mismo tag.
fn managed_dir_name(tag: &str, backend: Backend) -> String {
    let slug = match backend {
        Backend::Cpu { isa } => match isa {
            CpuIsa::Generic => "cpu".to_string(),
            CpuIsa::Avx => "cpu-avx".to_string(),
            CpuIsa::Avx2 => "cpu-avx2".to_string(),
            CpuIsa::Avx512 => "cpu-avx512".to_string(),
        },
        Backend::Cuda { major, minor } => format!("cuda-{major}.{minor}"),
        Backend::Vulkan => "vulkan".to_string(),
        Backend::Metal => "metal".to_string(),
    };
    format!("{tag}-{slug}")
}

/// `b<N>` (formato anterior) o `b<N>-<backend>`.
fn is_managed_dir_name(name: &str) -> bool {
    let tag = name.split_once('-').map_or(name, |(tag, _)| tag);
    build_number(tag).is_some()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProgressPhase {
    Fetching { artifact: &'static str },
    Extracting,
    Verifying,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ProgressEvent {
    pub phase: ProgressPhase,
    pub done: u64,
    pub total: u64,
}

impl ProgressEvent {
    pub fn percent(&self) -> u64 {
        (self.done * 100).checked_div(self.total).unwrap_or(0)
    }
}

/// Notifica solo cuando cambia la fase o el porcentaje entero (sin reloj: es determinista).
struct ProgressEmitter<'a> {
    sink: &'a (dyn Fn(ProgressEvent) + Send + Sync),
    last: Mutex<Option<(ProgressPhase, u64)>>,
}

impl<'a> ProgressEmitter<'a> {
    fn new(sink: &'a (dyn Fn(ProgressEvent) + Send + Sync)) -> Self {
        Self {
            sink,
            last: Mutex::new(None),
        }
    }

    fn emit(&self, phase: ProgressPhase, done: u64, total: u64) {
        let event = ProgressEvent { phase, done, total };
        let key = (phase, event.percent());
        let mut last = self.last.lock().unwrap_or_else(PoisonError::into_inner);
        if *last == Some(key) {
            return;
        }
        *last = Some(key);
        drop(last);
        (self.sink)(event);
    }
}

struct Archive {
    path: PathBuf,
    format: ArchiveFormat,
}

impl RuntimeManager {
    /// Instala (o actualiza) la última build compatible. Ante cualquier error o cancelación se
    /// borra el staging y el estado no cambia.
    pub async fn install(
        &self,
        preference: BackendPreference,
        on_progress: &(dyn Fn(ProgressEvent) + Send + Sync),
        cancel: &CancelToken,
    ) -> Result<ActiveRuntime, RuntimeError> {
        let _installing = self.begin_install()?;
        // `verify_server` necesita una ruta absoluta.
        let root = std::path::absolute(&self.runtime_dir)?;
        let emitter = ProgressEmitter::new(on_progress);
        self.remove_orphan_staging(&root).await?;
        let plan = self.plan_install(preference).await?;
        let final_dir = root.join(managed_dir_name(&plan.tag, plan.backend));
        if let Some(active) = self.active_if_installed(&plan).await? {
            emitter.emit(ProgressPhase::Done, 1, 1);
            return Ok(active);
        }
        let staging = root.join(format!("{STAGING_PREFIX}{}", plan.tag));
        // Si el paso 0 no pudo borrarlo, reutilizarlo mezclaría contenido viejo y nuevo.
        if tokio::fs::try_exists(&staging).await? {
            return Err(RuntimeError::Io(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "no se pudo limpiar el staging anterior",
            )));
        }
        let staged = self
            .install_staged(&plan, &final_dir, &staging, &emitter, cancel)
            .await;
        // Si el borrado falla, el directorio queda huérfano y el paso 0 de la siguiente
        // instalación lo retira; no debe tapar el resultado de la instalación.
        let _ = remove_dirs(Arc::clone(&self.deps.remover), vec![staging]).await;
        let active = staged?;
        // Mejor esfuerzo: el runtime nuevo ya está activo y lo viejo es solo espacio en disco.
        let _ = self.remove_superseded(&root, &final_dir).await;
        emitter.emit(ProgressPhase::Done, 1, 1);
        Ok(active)
    }

    async fn remove_orphan_staging(&self, root: &Path) -> Result<(), RuntimeError> {
        let orphans = list_dirs(root, |name| name.starts_with(STAGING_PREFIX)).await?;
        remove_dirs(Arc::clone(&self.deps.remover), orphans).await?;
        Ok(())
    }

    async fn plan_install(
        &self,
        preference: BackendPreference,
    ) -> Result<InstallPlan, RuntimeError> {
        let releases = self.deps.source.list_releases().await?;
        let hardware = self.hardware().await?;
        let release = latest_build_release(&releases, |release| is_compatible(release, &hardware))?;
        select_install(release, &hardware, preference)
    }

    /// El runtime activo si ya es esta misma build y backend y su servidor sigue en disco.
    async fn active_if_installed(
        &self,
        plan: &InstallPlan,
    ) -> Result<Option<ActiveRuntime>, RuntimeError> {
        let state = self.modify_state(|_| {}).await?;
        let Some(active) = state.active else {
            return Ok(None);
        };
        let same_build = matches!(&active.source, RuntimeSource::Managed { tag } if *tag == plan.tag)
            && active.backend == Some(plan.backend);
        if same_build && tokio::fs::try_exists(&active.server_path).await? {
            Ok(Some(active))
        } else {
            Ok(None)
        }
    }

    /// Descarga, extrae, verifica y activa la build; deja el resultado en `final_dir`.
    async fn install_staged(
        &self,
        plan: &InstallPlan,
        final_dir: &Path,
        staging: &Path,
        emitter: &ProgressEmitter<'_>,
        cancel: &CancelToken,
    ) -> Result<ActiveRuntime, RuntimeError> {
        let downloads = staging.join("downloads");
        let install_dir = staging.join("install");
        tokio::fs::create_dir_all(&downloads).await?;
        let main = self
            .download(&plan.main, "main", &downloads, emitter, cancel)
            .await?;
        let cudart = match &plan.cudart {
            Some(asset) => Some(
                self.download(asset, "cudart", &downloads, emitter, cancel)
                    .await?,
            ),
            None => None,
        };
        emitter.emit(ProgressPhase::Extracting, 0, 1);
        let os = self.deps.probe.os();
        let server = extract_runtime(main, cudart, install_dir.clone(), os, cancel.clone()).await?;
        let runner = self.deps.runner.as_ref();
        remove_quarantine(runner, os, &install_dir).await?;
        emitter.emit(ProgressPhase::Verifying, 0, 1);
        let version = verify_server(runner, &install_dir.join(&server)).await?;
        cancel.check()?;
        self.clear_residual(final_dir).await?;
        tokio::fs::rename(&install_dir, final_dir).await?;
        let active = ActiveRuntime {
            source: RuntimeSource::Managed {
                tag: plan.tag.clone(),
            },
            backend: Some(plan.backend),
            server_path: final_dir.join(server),
            version,
        };
        // Si no, el siguiente `status()` borraría el runtime recién activado.
        self.modify_state(|state| {
            state.active = Some(active.clone());
            state.pending_cleanup.retain(|path| path != final_dir);
        })
        .await?;
        Ok(active)
    }

    /// Un destino que ya existe (y no es el activo) es un residuo: se borra antes del `rename`.
    async fn clear_residual(&self, final_dir: &Path) -> Result<(), RuntimeError> {
        if !tokio::fs::try_exists(final_dir).await? {
            return Ok(());
        }
        let stuck = remove_dirs(
            Arc::clone(&self.deps.remover),
            vec![final_dir.to_path_buf()],
        )
        .await?;
        if stuck.is_empty() {
            return Ok(());
        }
        Err(RuntimeError::Io(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "no se pudo borrar el directorio residual de la build",
        )))
    }

    async fn download(
        &self,
        asset: &Asset,
        artifact: &'static str,
        dir: &Path,
        emitter: &ProgressEmitter<'_>,
        cancel: &CancelToken,
    ) -> Result<Archive, RuntimeError> {
        let format = classify_asset(&asset.name)
            .ok_or(RuntimeError::NoCompatibleAsset)?
            .format;
        let path = dir.join(&asset.name);
        let request = FetchRequest {
            url: asset.browser_download_url.clone(),
            dest: path.clone(),
            expected_size: asset.size,
            sha256: asset.sha256(),
        };
        let report = |done, total| emitter.emit(ProgressPhase::Fetching { artifact }, done, total);
        self.deps.fetcher.fetch(request, &report, cancel).await?;
        Ok(Archive { path, format })
    }

    /// Borra todas las builds gestionadas salvo `keep`, estuvieran activas o no. Las que no se
    /// pueden borrar (p. ej. un fichero bloqueado en Windows) quedan en `pending_cleanup`.
    async fn remove_superseded(&self, root: &Path, keep: &Path) -> Result<(), RuntimeError> {
        let keep_name = keep.file_name().map(|name| name.to_os_string());
        let old = list_dirs(root, move |name| {
            is_managed_dir_name(name) && keep_name.as_deref() != Some(name.as_ref())
        })
        .await?;
        let stuck = remove_dirs(Arc::clone(&self.deps.remover), old).await?;
        if stuck.is_empty() {
            return Ok(());
        }
        self.modify_state(|state| {
            for path in stuck {
                if !state.pending_cleanup.contains(&path) {
                    state.pending_cleanup.push(path);
                }
            }
        })
        .await?;
        Ok(())
    }
}

/// Extrae el principal y, junto a `llama-server`, el cudart. Devuelve la ruta del servidor
/// relativa a `install_dir`. La extracción es E/S bloqueante.
async fn extract_runtime(
    main: Archive,
    cudart: Option<Archive>,
    install_dir: PathBuf,
    os: Os,
    cancel: CancelToken,
) -> Result<PathBuf, RuntimeError> {
    tokio::task::spawn_blocking(move || {
        extract_runtime_blocking(&main, cudart.as_ref(), &install_dir, os, &cancel)
    })
    .await
    .map_err(join_failure)?
}

fn extract_runtime_blocking(
    main: &Archive,
    cudart: Option<&Archive>,
    install_dir: &Path,
    os: Os,
    cancel: &CancelToken,
) -> Result<PathBuf, RuntimeError> {
    extract_archive(&main.path, main.format, install_dir, cancel)?;
    let server = find_server_binary(install_dir, os)?;
    if let Some(cudart) = cudart {
        let server_dir = server.parent().unwrap_or(install_dir);
        extract_archive(&cudart.path, cudart.format, server_dir, cancel)?;
    }
    server
        .strip_prefix(install_dir)
        .map(Path::to_path_buf)
        .map_err(|error| RuntimeError::Extract(error.to_string()))
}

/// Subdirectorios de `root` cuyo nombre cumple `keep`; un `root` inexistente no tiene ninguno.
async fn list_dirs(
    root: &Path,
    keep: impl Fn(&str) -> bool + Send + 'static,
) -> Result<Vec<PathBuf>, RuntimeError> {
    let root = root.to_path_buf();
    tokio::task::spawn_blocking(move || list_dirs_blocking(&root, keep))
        .await
        .map_err(join_failure)?
}

fn list_dirs_blocking(
    root: &Path,
    keep: impl Fn(&str) -> bool,
) -> Result<Vec<PathBuf>, RuntimeError> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut dirs = Vec::new();
    for entry in entries {
        let entry = entry?;
        let selected = entry.file_name().to_str().is_some_and(&keep);
        if selected && entry.file_type()?.is_dir() {
            dirs.push(entry.path());
        }
    }
    Ok(dirs)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use tokio::sync::Notify;

    use super::*;
    use crate::runtime::test_support::{
        FakeFetcher, FakeProbe, FakeRemover, FakeRunner, FakeSource, Fixture, RELEASES,
        cuda_vulkan_probe, fixture_with_fetcher, managed, state_with, windows_probe,
    };
    use crate::runtime::{
        Arch, Backend, CpuIsa, Os, RuntimeSource, RuntimeState, RuntimeStatus, parse_releases,
    };

    const CPU_ZIP: &str = "llama-b12780-bin-win-cpu-x64.zip";
    const VERSION_OK: &str = "version: 12780 (abc1234)";

    fn windows_fetcher() -> FakeFetcher {
        FakeFetcher::new().with_archive(CPU_ZIP, &[("llama-server.exe", b"exe")])
    }

    fn windows_fixture(fetcher: FakeFetcher) -> Fixture {
        windows_fixture_with_remover(FakeRemover::deleting(), fetcher)
    }

    fn windows_fixture_with_remover(remover: FakeRemover, fetcher: FakeFetcher) -> Fixture {
        fixture_with_fetcher(
            FakeSource::with_releases(parse_releases(RELEASES).unwrap()),
            windows_probe(),
            FakeRunner::exits(0, "", VERSION_OK),
            remover,
            fetcher,
        )
    }

    fn install_fixture(probe: FakeProbe, runner: FakeRunner, fetcher: FakeFetcher) -> Fixture {
        fixture_with_fetcher(
            FakeSource::with_releases(parse_releases(RELEASES).unwrap()),
            probe,
            runner,
            FakeRemover::deleting(),
            fetcher,
        )
    }

    fn cuda_and_vulkan_fetcher() -> FakeFetcher {
        FakeFetcher::new()
            .with_archive(
                "llama-b12780-bin-win-cuda-13.5-x64.zip",
                &[("llama-server.exe", b"cuda")],
            )
            .with_archive(
                "cudart-llama-bin-win-cuda-13.5-x64.zip",
                &[("cudart64_13.dll", b"dll")],
            )
            .with_archive(
                "llama-b12780-bin-win-vulkan-x64.zip",
                &[("llama-server.exe", b"vulkan")],
            )
    }

    async fn install_with(
        fx: &Fixture,
        preference: BackendPreference,
    ) -> Result<ActiveRuntime, RuntimeError> {
        fx.manager
            .install(preference, &|_| {}, &CancelToken::default())
            .await
    }

    async fn install_auto(fx: &Fixture) -> Result<ActiveRuntime, RuntimeError> {
        fx.manager
            .install(BackendPreference::Auto, &|_| {}, &CancelToken::default())
            .await
    }

    fn dir_names(dir: &Path) -> Vec<String> {
        match std::fs::read_dir(dir) {
            Ok(entries) => entries
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    fn staging_dirs(fx: &Fixture) -> Vec<String> {
        dir_names(&fx.runtime_dir)
            .into_iter()
            .filter(|name| name.starts_with(".staging-"))
            .collect()
    }

    fn make_dir_with_file(dir: &Path) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("llama-server.exe"), "old").unwrap();
    }

    #[tokio::test]
    async fn install_cpu_windows_ok() {
        let fx = windows_fixture(windows_fetcher());
        let active = install_auto(&fx).await.unwrap();
        let server = fx.runtime_dir.join("b12780-cpu").join("llama-server.exe");
        assert!(server.is_file());
        assert_eq!(active.server_path, server);
        assert_eq!(active.version.build, 12780);
        let state = fx.saved();
        assert_eq!(state.active, Some(active));
        let active = state.active.unwrap();
        assert_eq!(
            active.source,
            RuntimeSource::Managed {
                tag: "b12780".to_string()
            }
        );
        assert_eq!(
            active.backend,
            Some(Backend::Cpu {
                isa: CpuIsa::Generic
            })
        );
        assert!(staging_dirs(&fx).is_empty());
    }

    #[tokio::test]
    async fn install_cuda_extrae_cudart_junto_al_server() {
        let fetcher = FakeFetcher::new()
            .with_archive(
                "llama-b12780-bin-win-cuda-13.5-x64.zip",
                &[("bin/llama-server.exe", b"exe")],
            )
            .with_archive(
                "cudart-llama-bin-win-cuda-13.5-x64.zip",
                &[("cudart64_13.dll", b"dll")],
            );
        let fx = install_fixture(
            cuda_vulkan_probe(),
            FakeRunner::exits(0, "", VERSION_OK),
            fetcher,
        );
        install_auto(&fx).await.unwrap();
        let bin = fx.runtime_dir.join("b12780-cuda-13.5").join("bin");
        assert!(bin.join("llama-server.exe").is_file());
        assert!(bin.join("cudart64_13.dll").is_file());
        assert_eq!(fx.fetcher.fetched().len(), 2);
    }

    #[tokio::test]
    async fn install_macos_quita_cuarentena_antes_de_verificar() {
        let probe = FakeProbe::new(Os::MacOs, Arch::Aarch64);
        let fetcher = FakeFetcher::new().with_archive(
            "llama-b12780-bin-macos-arm64.tar.gz",
            &[("llama-server", b"exe")],
        );
        let fx = install_fixture(probe, FakeRunner::exits(0, "", VERSION_OK), fetcher);
        install_auto(&fx).await.unwrap();
        let staging_install = fx.runtime_dir.join(".staging-b12780").join("install");
        let calls = fx.runner.calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].program, PathBuf::from("/usr/bin/xattr"));
        assert_eq!(
            calls[0].args,
            ["-cr".to_string(), staging_install.display().to_string()]
        );
        assert_eq!(calls[1].args, ["--version"]);
    }

    #[tokio::test]
    async fn install_windows_no_llama_xattr() {
        let fx = windows_fixture(windows_fetcher());
        install_auto(&fx).await.unwrap();
        let calls = fx.runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].args, ["--version"]);
    }

    #[tokio::test]
    async fn install_verificacion_falla_conserva_anterior() {
        let fx = install_fixture(
            windows_probe(),
            FakeRunner::exits(1, "", "missing cudart64_12.dll"),
            windows_fetcher(),
        );
        let previous = state_with(Some(managed(12779)));
        fx.save(&previous);
        make_dir_with_file(&fx.runtime_dir.join("b12779"));
        let result = install_auto(&fx).await;
        assert!(matches!(
            result,
            Err(RuntimeError::VerificationFailed { stderr_tail }) if stderr_tail.contains("cudart64_12")
        ));
        assert_eq!(fx.saved(), previous);
        assert!(
            fx.runtime_dir
                .join("b12779")
                .join("llama-server.exe")
                .is_file()
        );
        assert!(!fx.runtime_dir.join("b12780-cpu").exists());
        assert!(staging_dirs(&fx).is_empty());
    }

    #[tokio::test]
    async fn install_hash_falla_no_cambia_estado() {
        let fx = windows_fixture(windows_fetcher().failing_integrity());
        let previous = state_with(Some(managed(12779)));
        fx.save(&previous);
        assert!(matches!(
            install_auto(&fx).await,
            Err(RuntimeError::IntegrityMismatch)
        ));
        assert_eq!(fx.saved(), previous);
        assert!(staging_dirs(&fx).is_empty());
    }

    #[tokio::test]
    async fn install_cancelado() {
        let fx = windows_fixture(windows_fetcher().cancelling());
        assert!(matches!(
            install_auto(&fx).await,
            Err(RuntimeError::Cancelled)
        ));
        assert_eq!(fx.saved(), RuntimeState::default());
        assert!(staging_dirs(&fx).is_empty());
    }

    #[tokio::test]
    async fn install_cancelado_antes_de_extraer() {
        let fx = windows_fixture(windows_fetcher());
        let cancel = CancelToken::default();
        cancel.cancel();
        let result = fx
            .manager
            .install(BackendPreference::Auto, &|_| {}, &cancel)
            .await;
        assert!(matches!(result, Err(RuntimeError::Cancelled)));
        assert!(staging_dirs(&fx).is_empty());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn install_concurrente_rechazado() {
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let fetcher = windows_fetcher().gated(Arc::clone(&entered), Arc::clone(&release));
        let fx = windows_fixture(fetcher);
        let manager = Arc::clone(&fx.manager);
        let first = tokio::spawn(async move {
            manager
                .install(BackendPreference::Auto, &|_| {}, &CancelToken::default())
                .await
        });
        entered.notified().await;
        assert!(matches!(
            install_auto(&fx).await,
            Err(RuntimeError::AlreadyInstalling)
        ));
        release.notify_one();
        assert!(first.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn install_limpia_staging_huerfano() {
        let fx = windows_fixture(windows_fetcher());
        make_dir_with_file(&fx.runtime_dir.join(".staging-b12000"));
        install_auto(&fx).await.unwrap();
        assert!(staging_dirs(&fx).is_empty());
    }

    #[tokio::test]
    async fn install_progreso_por_fases() {
        let fx = windows_fixture(windows_fetcher());
        let events = std::sync::Mutex::new(Vec::new());
        fx.manager
            .install(
                BackendPreference::Auto,
                &|event| events.lock().unwrap().push(event),
                &CancelToken::default(),
            )
            .await
            .unwrap();
        let events = events.into_inner().unwrap();
        let mut phases: Vec<ProgressPhase> = events.iter().map(|event| event.phase).collect();
        phases.dedup();
        assert_eq!(
            phases,
            [
                ProgressPhase::Fetching { artifact: "main" },
                ProgressPhase::Extracting,
                ProgressPhase::Verifying,
                ProgressPhase::Done
            ]
        );
        assert!(
            events.windows(2).all(
                |pair| (pair[0].phase, pair[0].percent()) != (pair[1].phase, pair[1].percent())
            )
        );
    }

    #[tokio::test]
    async fn update_reemplaza_y_borra_anterior() {
        let fx = windows_fixture(windows_fetcher());
        fx.save(&state_with(Some(managed(12779))));
        make_dir_with_file(&fx.runtime_dir.join("b12779"));
        let active = install_auto(&fx).await.unwrap();
        assert_eq!(active.version.build, 12780);
        assert!(!fx.runtime_dir.join("b12779").exists());
        assert_eq!(fx.saved().active, Some(active));
    }

    #[tokio::test]
    async fn install_tras_externo_borra_gestionada_previa() {
        let fx = windows_fixture(windows_fetcher());
        fx.save(&state_with(Some(ActiveRuntime {
            source: RuntimeSource::External,
            backend: None,
            ..managed(12000)
        })));
        make_dir_with_file(&fx.runtime_dir.join("b12779"));
        install_auto(&fx).await.unwrap();
        assert!(!fx.runtime_dir.join("b12779").exists());
        assert!(matches!(
            fx.saved().active.map(|active| active.source),
            Some(RuntimeSource::Managed { .. })
        ));
    }

    #[tokio::test]
    async fn update_borrado_fallido_queda_pendiente() {
        let fx = windows_fixture_with_remover(FakeRemover::failing(), windows_fetcher());
        fx.save(&state_with(Some(managed(12779))));
        let old = fx.runtime_dir.join("b12779");
        make_dir_with_file(&old);
        install_auto(&fx).await.unwrap();
        assert_eq!(fx.saved().pending_cleanup, vec![old.clone()]);
        let retry = fx.restarted_manager();
        retry.status().await.unwrap();
        assert!(!old.exists());
        assert!(fx.saved().pending_cleanup.is_empty());
    }

    #[tokio::test]
    async fn status_persistente_entre_instancias() {
        let fx = windows_fixture(windows_fetcher());
        let active = install_auto(&fx).await.unwrap();
        let again = fx.restarted_manager();
        assert_eq!(again.status().await.unwrap(), RuntimeStatus::Ready(active));
    }

    #[test]
    fn managed_dir_name_por_backend() {
        let name = |backend| managed_dir_name("b12780", backend);
        let cpu = |isa| Backend::Cpu { isa };
        assert_eq!(name(cpu(CpuIsa::Generic)), "b12780-cpu");
        assert_eq!(name(cpu(CpuIsa::Avx)), "b12780-cpu-avx");
        assert_eq!(name(cpu(CpuIsa::Avx2)), "b12780-cpu-avx2");
        assert_eq!(name(cpu(CpuIsa::Avx512)), "b12780-cpu-avx512");
        assert_eq!(
            name(Backend::Cuda {
                major: 13,
                minor: 5
            }),
            "b12780-cuda-13.5"
        );
        assert_eq!(name(Backend::Vulkan), "b12780-vulkan");
        assert_eq!(name(Backend::Metal), "b12780-metal");
    }

    #[tokio::test]
    async fn reinstalar_mismo_tag_otro_backend() {
        let fx = install_fixture(
            cuda_vulkan_probe(),
            FakeRunner::exits(0, "", VERSION_OK),
            cuda_and_vulkan_fetcher(),
        );
        install_with(&fx, BackendPreference::Cuda).await.unwrap();
        let active = install_with(&fx, BackendPreference::Vulkan).await.unwrap();
        let vulkan = fx.runtime_dir.join("b12780-vulkan");
        assert_eq!(active.backend, Some(Backend::Vulkan));
        assert_eq!(active.server_path, vulkan.join("llama-server.exe"));
        assert!(active.server_path.is_file());
        assert!(!fx.runtime_dir.join("b12780-cuda-13.5").exists());
        assert_eq!(fx.saved().active, Some(active));
        assert!(staging_dirs(&fx).is_empty());
    }

    #[tokio::test]
    async fn reinstalar_mismo_tag_y_backend_no_descarga() {
        let fx = windows_fixture(windows_fetcher());
        let first = install_auto(&fx).await.unwrap();
        let fetched = fx.fetcher.fetched();
        let events = std::sync::Mutex::new(Vec::new());
        let again = fx
            .manager
            .install(
                BackendPreference::Auto,
                &|event| events.lock().unwrap().push(event.phase),
                &CancelToken::default(),
            )
            .await
            .unwrap();
        assert_eq!(again, first);
        assert_eq!(fx.fetcher.fetched(), fetched);
        assert_eq!(events.into_inner().unwrap(), [ProgressPhase::Done]);
        assert!(first.server_path.is_file());
    }

    #[tokio::test]
    async fn install_destino_residual_se_reemplaza() {
        let fx = windows_fixture(windows_fetcher());
        let residual = fx.runtime_dir.join("b12780-cpu");
        make_dir_with_file(&residual);
        std::fs::write(residual.join("stale.txt"), "old").unwrap();
        let active = install_auto(&fx).await.unwrap();
        assert_eq!(active.server_path, residual.join("llama-server.exe"));
        assert_eq!(std::fs::read(&active.server_path).unwrap(), b"exe");
        assert!(!residual.join("stale.txt").exists());
    }

    #[tokio::test]
    async fn install_destino_residual_no_borrable_falla() {
        let fx = windows_fixture_with_remover(FakeRemover::failing(), windows_fetcher());
        let previous = state_with(Some(managed(12779)));
        fx.save(&previous);
        make_dir_with_file(&fx.runtime_dir.join("b12780-cpu"));
        assert!(matches!(install_auto(&fx).await, Err(RuntimeError::Io(_))));
        assert_eq!(fx.saved(), previous);
    }

    #[tokio::test]
    async fn install_saca_destino_de_pending_cleanup() {
        let fx = windows_fixture(windows_fetcher());
        let target = fx.runtime_dir.join("b12780-cpu");
        make_dir_with_file(&target);
        fx.save(&RuntimeState {
            pending_cleanup: vec![target.clone()],
            ..RuntimeState::default()
        });
        let active = install_auto(&fx).await.unwrap();
        assert!(fx.saved().pending_cleanup.is_empty());
        let restarted = fx.restarted_manager();
        assert_eq!(
            restarted.status().await.unwrap(),
            RuntimeStatus::Ready(active)
        );
        assert!(target.join("llama-server.exe").is_file());
    }

    #[tokio::test]
    async fn install_staging_no_borrable_falla_antes_de_descargar() {
        let fx = windows_fixture_with_remover(FakeRemover::failing(), windows_fetcher());
        make_dir_with_file(&fx.runtime_dir.join(".staging-b12780"));
        assert!(matches!(install_auto(&fx).await, Err(RuntimeError::Io(_))));
        assert!(fx.fetcher.fetched().is_empty());
        assert_eq!(fx.saved(), RuntimeState::default());
    }

    #[tokio::test]
    async fn reinstalar_activo_roto_reemplaza() {
        let fx = windows_fixture(windows_fetcher());
        let first = install_auto(&fx).await.unwrap();
        std::fs::remove_file(&first.server_path).unwrap();
        let active = install_auto(&fx).await.unwrap();
        assert_eq!(active.server_path, first.server_path);
        assert!(active.server_path.is_file());
        assert_eq!(fx.fetcher.fetched().len(), 2);
        assert_eq!(fx.saved().active, Some(active));
    }
}
