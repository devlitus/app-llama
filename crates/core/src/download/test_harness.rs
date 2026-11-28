//! Banco de pruebas de los tests que ejecutan unidades de verdad: un gestor
//! con relojes, disco y almacén falsos, y un servidor local de por medio.
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::Url;
use sha2::{Digest, Sha256};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::events::{DownloadProgress, DownloadSnapshot, EventSink, RecordingSink};
use super::paths::part_path;
use super::request::{PartRequest, UnitRequest};
use super::test_support::FakeTokens;
use super::{
    Clock, DownloadId, DownloadManager, DownloadUnit, FakeClock, FakeDiskSpace, FakeSleeper,
    FileSpec, FileStatus, HttpFetcher, JsonQueueStore, ManagerDeps, QueueFile, QueuePersistence,
    RecordingStore, UnitState,
};
use crate::hf::RepoId;

pub(super) const KIB: usize = 1024;
pub(super) const MIB: usize = 1024 * KIB;
pub(super) const GIB: u64 = 1024 * 1024 * 1024;
pub(super) const REVISION: &str = "9217f5db79a29953eb74d5343926648285ec7e67";
/// Cota de seguridad de las esperas activas: solo se agota si el test falla.
pub(super) const WAIT_LIMIT: Duration = Duration::from_secs(30);

pub(super) enum SinkEvent<'a> {
    State(&'a DownloadSnapshot),
    Progress(&'a DownloadProgress),
}

pub(super) type Hook = Arc<dyn Fn(SinkEvent<'_>) + Send + Sync>;
pub(super) type HookFactory = Box<dyn FnOnce(&Path) -> Hook>;

/// Dónde vive la cola de un test, para construir sus unidades a mano.
pub(super) struct QueueEnv<'a> {
    pub models_dir: &'a Path,
    pub base: &'a str,
}

impl QueueEnv<'_> {
    /// Una parte de `org/model` cuyo contenido es `body` (su `sha256` es el real).
    pub(super) fn file(&self, relative: &str, body: &[u8], status: FileStatus) -> FileSpec {
        FileSpec {
            relative_path: relative.to_string(),
            url: format!(
                "{}/org/model/resolve/{REVISION}/{relative}",
                self.base.trim_end_matches('/')
            ),
            dest: self.models_dir.join("org").join("model").join(relative),
            expected_size: body.len() as u64,
            sha256: Some(sha_hex(body)),
            status,
        }
    }
}

/// Una unidad ya creada, con el estado que dejaría una sesión anterior.
pub(super) fn unit_of(id: u64, state: UnitState, files: Vec<FileSpec>) -> DownloadUnit {
    DownloadUnit {
        id: DownloadId(id),
        repo_id: "org/model".to_string(),
        revision: REVISION.to_string(),
        display_name: "m".to_string(),
        files,
        state,
    }
}

/// Cola con la que arranca un test, a partir de las rutas del entorno.
pub(super) type QueueFactory = Box<dyn FnOnce(&QueueEnv<'_>) -> Vec<DownloadUnit>>;

/// Registra los eventos y deja que un test reaccione a cada uno.
struct HookedSink {
    recorder: Arc<RecordingSink>,
    hook: Hook,
}

impl EventSink for HookedSink {
    fn state(&self, snapshot: &DownloadSnapshot) {
        EventSink::state(self.recorder.as_ref(), snapshot);
        (self.hook)(SinkEvent::State(snapshot));
    }

    fn progress(&self, progress: &DownloadProgress) {
        EventSink::progress(self.recorder.as_ref(), progress);
        (self.hook)(SinkEvent::Progress(progress));
    }
}

pub(super) struct Options {
    pub disk: u64,
    pub clock: Arc<dyn Clock>,
    pub hook: Option<HookFactory>,
    /// Unidades que ya estaban en `downloads.json` al arrancar el gestor.
    pub queue: Option<QueueFactory>,
    pub max_active: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            disk: u64::MAX,
            clock: Arc::new(FakeClock::new()),
            hook: None,
            queue: None,
            max_active: 1,
        }
    }
}

pub(super) struct Harness {
    pub dir: tempfile::TempDir,
    pub store: Arc<RecordingStore>,
    pub sink: Arc<RecordingSink>,
    pub manager: Arc<DownloadManager>,
}

impl Harness {
    pub(super) fn new(base: &str) -> Self {
        Self::configured(base, Options::default())
    }

    pub(super) fn configured(base: &str, options: Options) -> Self {
        let dir = tempfile::tempdir().expect("tmp");
        let queue = options
            .queue
            .map(|factory| {
                let env = QueueEnv {
                    models_dir: dir.path(),
                    base,
                };
                restored(&dir.path().join("state"), factory(&env))
            })
            .unwrap_or_default();
        let store = Arc::new(RecordingStore::with_queue(queue));
        let recorder = Arc::new(RecordingSink::default());
        let hook: Hook = match options.hook {
            Some(factory) => factory(dir.path()),
            None => Arc::new(|_| {}),
        };
        let base_url = Url::parse(base).expect("url base");
        let manager = DownloadManager::new(ManagerDeps {
            fetcher: Arc::new(HttpFetcher::new(
                reqwest::Client::new(),
                Some(base_url.clone()),
            )),
            clock: options.clock,
            sleeper: Arc::new(FakeSleeper::new()),
            disk: Arc::new(FakeDiskSpace(Some(options.disk))),
            tokens: Arc::new(FakeTokens::new(None)),
            store: store.clone(),
            sink: Arc::new(HookedSink {
                recorder: recorder.clone(),
                hook,
            }),
            models_dir: dir.path().to_path_buf(),
            hf_base_url: base_url,
            max_active: options.max_active,
        })
        .expect("gestor");
        Self {
            dir,
            store,
            sink: recorder,
            manager,
        }
    }

    pub(super) fn dest(&self, relative: &str) -> PathBuf {
        self.dir.path().join("org").join("model").join(relative)
    }

    pub(super) fn write_final(&self, relative: &str, bytes: &[u8]) {
        let dest = self.dest(relative);
        fs::create_dir_all(dest.parent().expect("padre")).expect("directorio");
        fs::write(dest, bytes).expect("final");
    }

    pub(super) fn write_part(&self, relative: &str, bytes: &[u8]) {
        let dest = part_path(&self.dest(relative));
        fs::create_dir_all(dest.parent().expect("padre")).expect("directorio");
        fs::write(dest, bytes).expect("parcial");
    }

    /// Longitud del `.part` de `relative`, o `None` si no existe.
    pub(super) fn part_len(&self, relative: &str) -> Option<u64> {
        fs::metadata(part_path(&self.dest(relative)))
            .ok()
            .map(|metadata| metadata.len())
    }

    pub(super) fn part_exists(&self, relative: &str) -> bool {
        self.part_len(relative).is_some()
    }

    pub(super) async fn enqueue(&self, parts: Vec<PartRequest>) -> DownloadId {
        let request = UnitRequest {
            repo: RepoId::parse("org/model").expect("repo"),
            revision: REVISION.to_string(),
            parts,
        };
        self.manager.enqueue(request).await.expect("alta")
    }

    pub(super) async fn state_of(&self, id: DownloadId) -> UnitState {
        let units = self.manager.list().await;
        units
            .into_iter()
            .find(|unit| unit.id == id)
            .map(|unit| unit.state)
            .expect("unidad")
    }

    /// Espera, cediendo el turno, a que `done` se cumpla.
    pub(super) async fn wait_until(&self, what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + WAIT_LIMIT;
        while Instant::now() < deadline {
            if done() {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("no se cumplió: {what}");
    }

    /// Espera a que el `.part` de `relative` llegue a `len` bytes.
    pub(super) async fn wait_for_part(&self, relative: &str, len: u64) {
        self.wait_until(&format!("{relative} con {len} bytes"), || {
            self.part_len(relative).is_some_and(|actual| actual >= len)
        })
        .await;
    }

    /// Espera, cediendo el turno, a que la unidad llegue a un estado en reposo.
    pub(super) async fn finished(&self, id: DownloadId) -> UnitState {
        let deadline = Instant::now() + WAIT_LIMIT;
        while Instant::now() < deadline {
            let state = self.state_of(id).await;
            if !matches!(
                state,
                UnitState::Queued | UnitState::Downloading | UnitState::Verifying
            ) {
                return state;
            }
            tokio::task::yield_now().await;
        }
        panic!("la unidad {id:?} no terminó");
    }

    pub(super) fn states(&self, id: DownloadId) -> Vec<UnitState> {
        self.sink
            .states()
            .into_iter()
            .filter(|snapshot| snapshot.id == id)
            .map(|snapshot| snapshot.state)
            .collect()
    }

    pub(super) async fn file_status(&self, id: DownloadId, index: usize) -> FileStatus {
        let units = self.manager.list().await;
        let unit = units.iter().find(|unit| unit.id == id).expect("unidad");
        unit.files[index].status
    }
}

/// Guarda la cola como `downloads.json` y la vuelve a cargar, como al arrancar
/// la app: lo que estaba en marcha queda en pausa.
fn restored(dir: &Path, units: Vec<DownloadUnit>) -> QueueFile {
    let next_id = units
        .iter()
        .map(|unit| unit.id.0)
        .max()
        .map_or(1, |id| id + 1);
    let store = JsonQueueStore::new(dir);
    let queue = QueueFile {
        next_id,
        units,
        ..QueueFile::default()
    };
    store.save(&queue).expect("guardar downloads.json");
    store.load().expect("cargar downloads.json")
}

pub(super) fn sha_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(super) fn part(name: &str, body: &[u8]) -> PartRequest {
    PartRequest {
        relative_path: name.to_string(),
        size: body.len() as u64,
        sha256: Some(sha_hex(body)),
    }
}

pub(super) fn part_without_sha(name: &str, size: u64) -> PartRequest {
    PartRequest {
        relative_path: name.to_string(),
        size,
        sha256: None,
    }
}

pub(super) async fn serve(server: &MockServer, name: &str, body: &[u8]) {
    Mock::given(method("GET"))
        .and(path(format!("/org/model/resolve/{REVISION}/{name}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(server)
        .await;
}

pub(super) async fn request_count(server: &MockServer) -> usize {
    server.received_requests().await.expect("peticiones").len()
}
