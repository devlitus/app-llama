//! Dobles de test compartidos por los módulos de `runtime`.
use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::Notify;

use crate::runtime::{
    ActiveRuntime, Arch, ArtifactFetcher, Backend, CancelToken, CommandOutput, CommandRunner,
    CpuIsa, DirRemover, FetchRequest, Os, Release, ReleaseSource, RuntimeDeps, RuntimeError,
    RuntimeManager, RuntimeSource, RuntimeState, RuntimeVersion, StateStore, SystemProbe,
    parse_releases,
};

pub const RELEASES: &str = include_str!(concat!(
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedCall {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

enum Reply {
    Output(CommandOutput),
    LaunchError,
}

/// `CommandRunner` que no lanza procesos: registra las llamadas y responde siempre igual.
pub struct FakeRunner {
    reply: Reply,
    calls: Mutex<Vec<RecordedCall>>,
}

impl FakeRunner {
    fn with_reply(reply: Reply) -> Self {
        Self {
            reply,
            calls: Mutex::new(Vec::new()),
        }
    }

    pub fn exits(code: i32, stdout: &str, stderr: &str) -> Self {
        Self::with_reply(Reply::Output(CommandOutput {
            exit_code: Some(code),
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
        }))
    }

    pub fn fails_to_launch() -> Self {
        Self::with_reply(Reply::LaunchError)
    }

    pub fn calls(&self) -> Vec<RecordedCall> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

#[async_trait::async_trait]
impl CommandRunner for FakeRunner {
    async fn run(
        &self,
        program: &Path,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> Result<CommandOutput, RuntimeError> {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(RecordedCall {
                program: program.to_path_buf(),
                args: args.iter().map(ToString::to_string).collect(),
                cwd: cwd.map(Path::to_path_buf),
            });
        match &self.reply {
            Reply::Output(output) => Ok(output.clone()),
            Reply::LaunchError => Err(RuntimeError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no se pudo lanzar",
            ))),
        }
    }
}

pub struct FakeProbe {
    os: Os,
    arch: Arch,
    cpu_features: HashSet<&'static str>,
    outputs: HashMap<String, String>,
    existing_files: HashSet<PathBuf>,
    system_root: Option<PathBuf>,
    ram_bytes: Option<u64>,
    runs: Mutex<Vec<String>>,
}

impl FakeProbe {
    pub fn new(os: Os, arch: Arch) -> Self {
        Self {
            os,
            arch,
            cpu_features: HashSet::new(),
            outputs: HashMap::new(),
            existing_files: HashSet::new(),
            system_root: None,
            ram_bytes: None,
            runs: Mutex::new(Vec::new()),
        }
    }

    pub fn with_cpu(mut self, features: &[&'static str]) -> Self {
        self.cpu_features.extend(features);
        self
    }

    pub fn with_output(mut self, program: &str, args: &[&str], stdout: &str) -> Self {
        self.outputs
            .insert(command_line(program, args), stdout.to_string());
        self
    }

    pub fn with_file(mut self, path: PathBuf) -> Self {
        self.existing_files.insert(path);
        self
    }

    pub fn with_system_root(mut self, root: PathBuf) -> Self {
        self.system_root = Some(root);
        self
    }

    pub fn with_ram(mut self, bytes: u64) -> Self {
        self.ram_bytes = Some(bytes);
        self
    }

    pub fn run_count(&self) -> usize {
        self.runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }
}

fn command_line(program: &str, args: &[&str]) -> String {
    std::iter::once(program)
        .chain(args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ")
}

impl SystemProbe for FakeProbe {
    fn os(&self) -> Os {
        self.os
    }

    fn arch(&self) -> Arch {
        self.arch
    }

    fn cpu_has(&self, feature: &str) -> bool {
        self.cpu_features.contains(feature)
    }

    fn run(&self, program: &str, args: &[&str]) -> Option<String> {
        let line = command_line(program, args);
        self.runs
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line.clone());
        self.outputs.get(&line).cloned()
    }

    fn file_exists(&self, path: &Path) -> bool {
        self.existing_files.contains(path)
    }

    fn system_root(&self) -> Option<PathBuf> {
        self.system_root.clone()
    }

    fn total_memory_bytes(&self) -> Option<u64> {
        self.ram_bytes
    }
}

enum SourceReply {
    Releases(Vec<Release>),
    RateLimited,
}

/// `ReleaseSource` que responde siempre lo mismo, sin red.
pub struct FakeSource {
    reply: SourceReply,
}

impl FakeSource {
    pub fn with_releases(releases: Vec<Release>) -> Self {
        Self {
            reply: SourceReply::Releases(releases),
        }
    }

    pub fn rate_limited() -> Self {
        Self {
            reply: SourceReply::RateLimited,
        }
    }
}

#[async_trait::async_trait]
impl ReleaseSource for FakeSource {
    async fn list_releases(&self) -> Result<Vec<Release>, RuntimeError> {
        match &self.reply {
            SourceReply::Releases(releases) => Ok(releases.clone()),
            SourceReply::RateLimited => Err(RuntimeError::RateLimited { reset_epoch: None }),
        }
    }
}

type ArchiveEntries = Vec<(String, Vec<u8>)>;

enum FetchBehavior {
    Serve,
    FailIntegrity,
    Cancel,
    /// Avisa de que ha entrado y espera a que el test le deje continuar.
    Gate {
        entered: Arc<Notify>,
        release: Arc<Notify>,
    },
}

/// `ArtifactFetcher` sin red: escribe en `dest` un zip o tar.gz generado con las entradas
/// configuradas para el nombre del fichero (el formato lo decide la extensión).
pub struct FakeFetcher {
    archives: HashMap<String, ArchiveEntries>,
    behavior: FetchBehavior,
    fetched: Mutex<Vec<String>>,
}

impl FakeFetcher {
    pub fn new() -> Self {
        Self {
            archives: HashMap::new(),
            behavior: FetchBehavior::Serve,
            fetched: Mutex::new(Vec::new()),
        }
    }

    pub fn with_archive(mut self, file_name: &str, entries: &[(&str, &[u8])]) -> Self {
        let entries = entries
            .iter()
            .map(|(path, content)| (path.to_string(), content.to_vec()))
            .collect();
        self.archives.insert(file_name.to_string(), entries);
        self
    }

    pub fn failing_integrity(mut self) -> Self {
        self.behavior = FetchBehavior::FailIntegrity;
        self
    }

    pub fn cancelling(mut self) -> Self {
        self.behavior = FetchBehavior::Cancel;
        self
    }

    pub fn gated(mut self, entered: Arc<Notify>, release: Arc<Notify>) -> Self {
        self.behavior = FetchBehavior::Gate { entered, release };
        self
    }

    /// Nombres de los ficheros pedidos, en orden.
    pub fn fetched(&self) -> Vec<String> {
        self.fetched
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

fn write_archive(dest: &Path, entries: &ArchiveEntries) -> std::io::Result<()> {
    let file = std::fs::File::create(dest)?;
    if dest.extension().is_some_and(|ext| ext == "zip") {
        write_zip(file, entries)
    } else {
        write_targz(file, entries)
    }
}

fn write_zip(file: std::fs::File, entries: &ArchiveEntries) -> std::io::Result<()> {
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (path, content) in entries {
        writer
            .start_file(path, options)
            .map_err(std::io::Error::other)?;
        std::io::Write::write_all(&mut writer, content)?;
    }
    writer.finish().map_err(std::io::Error::other)?;
    Ok(())
}

fn write_targz(file: std::fs::File, entries: &ArchiveEntries) -> std::io::Result<()> {
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    let mut builder = tar::Builder::new(encoder);
    for (path, content) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append_data(&mut header, path, content.as_slice())?;
    }
    builder.into_inner()?.finish()?;
    Ok(())
}

#[async_trait::async_trait]
impl ArtifactFetcher for FakeFetcher {
    async fn fetch(
        &self,
        req: FetchRequest,
        progress: &(dyn Fn(u64, u64) + Send + Sync),
        cancel: &CancelToken,
    ) -> Result<(), RuntimeError> {
        let name = req
            .dest
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        self.fetched
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(name.clone());
        match &self.behavior {
            FetchBehavior::Serve => {}
            FetchBehavior::FailIntegrity => return Err(RuntimeError::IntegrityMismatch),
            FetchBehavior::Cancel => {
                cancel.cancel();
                return Err(RuntimeError::Cancelled);
            }
            FetchBehavior::Gate { entered, release } => {
                entered.notify_one();
                release.notified().await;
            }
        }
        let entries = self.archives.get(&name).ok_or_else(|| {
            RuntimeError::Io(std::io::Error::other(format!("sin archivo para {name}")))
        })?;
        write_archive(&req.dest, entries)?;
        // La repetición comprueba que el emisor de progreso descarta lo que no cambia.
        for (done, total) in [(50, 100), (50, 100), (100, 100)] {
            progress(done, total);
        }
        Ok(())
    }
}

enum RemoverMode {
    Deleting,
    FailingAlways,
    FailingFor(OsString),
}

/// `DirRemover` que borra de verdad, falla siempre o falla solo para una ruta concreta.
pub struct FakeRemover {
    mode: RemoverMode,
}

impl FakeRemover {
    pub fn deleting() -> Self {
        Self {
            mode: RemoverMode::Deleting,
        }
    }

    pub fn failing() -> Self {
        Self {
            mode: RemoverMode::FailingAlways,
        }
    }

    /// Falla solo para los directorios con ese nombre (el `tempdir` aún no existe al crearlo).
    pub fn failing_for(dir_name: &str) -> Self {
        Self {
            mode: RemoverMode::FailingFor(dir_name.into()),
        }
    }
}

impl DirRemover for FakeRemover {
    fn remove_dir_all(&self, path: &Path) -> std::io::Result<()> {
        let blocked = match &self.mode {
            RemoverMode::Deleting => false,
            RemoverMode::FailingAlways => true,
            RemoverMode::FailingFor(name) => path.file_name() == Some(name.as_os_str()),
        };
        if blocked {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "fichero bloqueado",
            ));
        }
        std::fs::remove_dir_all(path)
    }
}

pub fn windows_probe() -> FakeProbe {
    FakeProbe::new(Os::Windows, Arch::X86_64).with_cpu(&["avx2"])
}

/// Windows x64 con una RTX 4070 (CUDA 13.0) y Vulkan.
pub fn cuda_vulkan_probe() -> FakeProbe {
    let root = PathBuf::from("sysroot");
    let vulkan_dll = root.join("System32").join("vulkan-1.dll");
    windows_probe()
        .with_output("nvidia-smi", &GPU_ARGS, "NVIDIA GeForce RTX 4070, 12282\n")
        .with_output("nvidia-smi", &[], SMI_HEADER)
        .with_system_root(root)
        .with_file(vulkan_dll)
}

pub fn managed(build: u32) -> ActiveRuntime {
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

pub fn state_with(active: Option<ActiveRuntime>) -> RuntimeState {
    RuntimeState {
        active,
        ..RuntimeState::default()
    }
}

/// `RuntimeManager` sobre un `tempdir`, con los dobles accesibles para inspeccionarlos.
pub struct Fixture {
    _temp: tempfile::TempDir,
    pub runtime_dir: PathBuf,
    pub runner: Arc<FakeRunner>,
    pub fetcher: Arc<FakeFetcher>,
    pub manager: Arc<RuntimeManager>,
}

pub fn fixture(
    source: FakeSource,
    probe: FakeProbe,
    runner: FakeRunner,
    remover: FakeRemover,
) -> Fixture {
    fixture_with_fetcher(source, probe, runner, remover, FakeFetcher::new())
}

pub fn fixture_with_fetcher(
    source: FakeSource,
    probe: FakeProbe,
    runner: FakeRunner,
    remover: impl DirRemover + 'static,
    fetcher: FakeFetcher,
) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let runtime_dir = temp.path().join("runtime");
    let runner = Arc::new(runner);
    let fetcher = Arc::new(fetcher);
    let manager = Arc::new(RuntimeManager::new(
        &runtime_dir,
        RuntimeDeps {
            source: Arc::new(source),
            fetcher: fetcher.clone(),
            runner: runner.clone(),
            probe: Arc::new(probe),
            remover: Arc::new(remover),
        },
    ));
    Fixture {
        _temp: temp,
        runtime_dir,
        runner,
        fetcher,
        manager,
    }
}

pub fn default_fixture() -> Fixture {
    fixture(
        FakeSource::with_releases(parse_releases(RELEASES).unwrap()),
        windows_probe(),
        FakeRunner::exits(0, "", "version: 12000 (abc1234)"),
        FakeRemover::deleting(),
    )
}

impl Fixture {
    pub fn store(&self) -> StateStore {
        StateStore::new(&self.runtime_dir)
    }

    pub fn save(&self, state: &RuntimeState) {
        self.store().save(state).unwrap();
    }

    pub fn saved(&self) -> RuntimeState {
        self.store().load().unwrap()
    }

    /// Otro manager sobre el mismo directorio, como tras reiniciar la app: sin releases, sin
    /// descargas y con un `DirRemover` que borra de verdad.
    pub fn restarted_manager(&self) -> RuntimeManager {
        RuntimeManager::new(
            &self.runtime_dir,
            RuntimeDeps {
                source: Arc::new(FakeSource::with_releases(Vec::new())),
                fetcher: Arc::new(FakeFetcher::new()),
                runner: Arc::new(FakeRunner::exits(0, "", "")),
                probe: Arc::new(windows_probe()),
                remover: Arc::new(FakeRemover::deleting()),
            },
        )
    }
}
