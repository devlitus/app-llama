//! Gestor de la cola de descargas: alta de unidades y consulta de su estado.
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use reqwest::Url;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use super::auth::TokenProvider;
use super::clock::Clock;
use super::disk::DiskSpace;
use super::error::DownloadError;
use super::events::{DownloadSnapshot, EventSink};
use super::model::{DownloadId, DownloadUnit, FileSpec, FileStatus, UnitState};
use super::paths::safe_join;
use super::request::UnitRequest;
use super::retry::Sleeper;
use super::runner::run_unit;
use super::split::{display_name, validate_parts};
use super::store::{QueueFile, QueuePersistence};
use super::transfer::HttpFetcher;
use crate::hf::url::resolve_url;

/// Todo lo que el gestor necesita del exterior, inyectado para poder probarlo.
pub struct ManagerDeps {
    pub fetcher: Arc<HttpFetcher>,
    pub clock: Arc<dyn Clock>,
    pub sleeper: Arc<dyn Sleeper>,
    pub disk: Arc<dyn DiskSpace>,
    pub tokens: Arc<dyn TokenProvider>,
    pub store: Arc<dyn QueuePersistence>,
    pub sink: Arc<dyn EventSink>,
    pub models_dir: PathBuf,
    pub hf_base_url: Url,
    pub max_active: usize,
}

struct QueueState {
    units: Vec<DownloadUnit>,
    next_id: u64,
    version: u64,
    /// Unidades con una ejecución lanzada, con el token que la cancela.
    active: HashMap<DownloadId, CancellationToken>,
}

impl QueueState {
    fn to_file(&self) -> QueueFile {
        QueueFile {
            next_id: self.next_id,
            units: self.units.clone(),
            ..QueueFile::default()
        }
    }

    fn unit_mut(&mut self, id: DownloadId) -> Result<&mut DownloadUnit, DownloadError> {
        self.units
            .iter_mut()
            .find(|unit| unit.id == id)
            .ok_or(DownloadError::UnknownDownload)
    }

    /// La `Queued` de menor id que aún no tiene ejecución lanzada.
    fn next_waiting(&self) -> Option<DownloadId> {
        self.units
            .iter()
            .filter(|unit| unit.state == UnitState::Queued && !self.active.contains_key(&unit.id))
            .map(|unit| unit.id)
            .min()
    }
}

pub struct DownloadManager {
    pub(super) deps: ManagerDeps,
    // El cerrojo se mantiene durante `save`: las escrituras del almacén deben ir serializadas.
    queue: Mutex<QueueState>,
}

impl DownloadManager {
    pub fn new(deps: ManagerDeps) -> Result<Arc<Self>, DownloadError> {
        let saved = deps.store.load()?;
        let queue = QueueState {
            units: saved.units,
            next_id: saved.next_id,
            version: 0,
            active: HashMap::new(),
        };
        Ok(Arc::new(Self {
            deps,
            queue: Mutex::new(queue),
        }))
    }

    pub async fn list(&self) -> Vec<DownloadSnapshot> {
        let queue = self.queue.lock().await;
        queue
            .units
            .iter()
            .map(|unit| DownloadSnapshot::of(unit, queue.version))
            .collect()
    }

    pub async fn enqueue(
        self: &Arc<Self>,
        request: UnitRequest,
    ) -> Result<DownloadId, DownloadError> {
        let files = self.build_files(&request)?;
        reject_repeated_destinations(&files)?;
        reject_conflicting_finals(&files)?;
        let mut queue = self.queue.lock().await;
        reject_taken_destinations(&queue.units, &files)?;
        let unit = DownloadUnit {
            id: DownloadId(queue.next_id),
            repo_id: request.repo.as_str().to_owned(),
            revision: request.revision,
            display_name: display_name(&request.parts[0].relative_path),
            files,
            state: UnitState::Queued,
        };
        let id = unit.id;
        self.append_and_persist(&mut queue, unit)?;
        self.start_waiting(&mut queue);
        Ok(id)
    }

    /// Lanza las unidades en cola, por orden de id, mientras quede hueco.
    fn start_waiting(self: &Arc<Self>, queue: &mut QueueState) {
        while queue.active.len() < self.deps.max_active {
            let Some(id) = queue.next_waiting() else {
                break;
            };
            let cancel = CancellationToken::new();
            queue.active.insert(id, cancel.clone());
            tokio::spawn(run_unit(Arc::clone(self), id, cancel));
        }
    }

    /// Libera el hueco de una ejecución terminada y arranca la siguiente.
    pub(super) async fn finish_run(self: &Arc<Self>, id: DownloadId) {
        let mut queue = self.queue.lock().await;
        queue.active.remove(&id);
        self.start_waiting(&mut queue);
    }

    pub(super) async fn unit(&self, id: DownloadId) -> Result<DownloadUnit, DownloadError> {
        let mut queue = self.queue.lock().await;
        queue.unit_mut(id).map(|unit| unit.clone())
    }

    /// Cambia el estado de una unidad si la tabla de transiciones lo permite.
    /// Se persiste y se emite siempre, con los estados de sus ficheros. Si el
    /// guardado falla, el nuevo estado sigue en memoria y se devuelve el error.
    pub(super) async fn transition(
        &self,
        id: DownloadId,
        to: UnitState,
    ) -> Result<(), DownloadError> {
        let mut queue = self.queue.lock().await;
        let unit = queue.unit_mut(id)?;
        if !unit.state.can_transition(&to) {
            return Err(DownloadError::InvalidTransition);
        }
        unit.state = to;
        let changed = unit.clone();
        queue.version += 1;
        let saved = self.deps.store.save(&queue.to_file());
        self.deps
            .sink
            .state(&DownloadSnapshot::of(&changed, queue.version));
        saved
    }

    /// Anota el estado de un fichero. Se persiste con el siguiente cambio de
    /// estado de la unidad, nunca por sí solo.
    pub(super) async fn mark_file(&self, id: DownloadId, index: usize, status: FileStatus) {
        let mut queue = self.queue.lock().await;
        if let Some(file) = queue
            .unit_mut(id)
            .ok()
            .and_then(|unit| unit.files.get_mut(index))
        {
            file.status = status;
        }
    }

    fn build_files(&self, request: &UnitRequest) -> Result<Vec<FileSpec>, DownloadError> {
        let paths: Vec<&str> = request
            .parts
            .iter()
            .map(|part| part.relative_path.as_str())
            .collect();
        validate_parts(&paths)?;
        let repo_dir = request
            .repo
            .as_str()
            .split('/')
            .fold(self.deps.models_dir.clone(), |dir, segment| {
                dir.join(segment)
            });
        request
            .parts
            .iter()
            .map(|part| {
                let url = resolve_url(
                    &self.deps.hf_base_url,
                    &request.repo,
                    &request.revision,
                    &part.relative_path,
                );
                Ok(FileSpec {
                    relative_path: part.relative_path.clone(),
                    url: url.into(),
                    dest: safe_join(&repo_dir, &part.relative_path)?,
                    expected_size: part.size,
                    sha256: part.sha256.clone(),
                    status: FileStatus::Pending,
                })
            })
            .collect()
    }

    /// Añade la unidad, la persiste y emite su estado. Si no se puede guardar,
    /// la cola queda como estaba.
    fn append_and_persist(
        &self,
        queue: &mut QueueState,
        unit: DownloadUnit,
    ) -> Result<(), DownloadError> {
        queue.units.push(unit);
        queue.next_id += 1;
        queue.version += 1;
        if let Err(error) = self.deps.store.save(&queue.to_file()) {
            queue.units.pop();
            queue.next_id -= 1;
            queue.version -= 1;
            return Err(error);
        }
        if let Some(unit) = queue.units.last() {
            self.deps
                .sink
                .state(&DownloadSnapshot::of(unit, queue.version));
        }
        Ok(())
    }
}

/// Clave de comparación de un destino: Windows y macOS no distinguen
/// mayúsculas y Windows ignora los puntos y espacios finales de cada nombre.
/// Se pasa a mayúsculas carácter a carácter, como NTFS, porque `to_lowercase`
/// distingue `ς` de `σ` según la posición y NTFS los trata como el mismo.
pub(super) fn dest_key(path: &Path) -> String {
    path.components()
        .map(|component| {
            component
                .as_os_str()
                .to_string_lossy()
                .chars()
                .flat_map(char::to_uppercase)
                .collect::<String>()
                .trim_end_matches(['.', ' '])
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn reject_repeated_destinations(files: &[FileSpec]) -> Result<(), DownloadError> {
    let mut seen = HashSet::new();
    if files.iter().all(|file| seen.insert(dest_key(&file.dest))) {
        Ok(())
    } else {
        Err(DownloadError::AlreadyQueued)
    }
}

fn reject_taken_destinations(
    units: &[DownloadUnit],
    files: &[FileSpec],
) -> Result<(), DownloadError> {
    let taken: HashSet<String> = units
        .iter()
        .filter(|unit| unit.state.blocks_destination())
        .flat_map(|unit| &unit.files)
        .map(|file| dest_key(&file.dest))
        .collect();
    if files
        .iter()
        .any(|file| taken.contains(&dest_key(&file.dest)))
    {
        Err(DownloadError::AlreadyQueued)
    } else {
        Ok(())
    }
}

/// Un final con otro tamaño no es el fichero esperado y no se pisa. Con el
/// tamaño esperado queda como candidato a `Preexisting`: se verifica al ejecutar.
fn reject_conflicting_finals(files: &[FileSpec]) -> Result<(), DownloadError> {
    for file in files {
        match fs::metadata(&file.dest) {
            Ok(metadata) if metadata.len() != file.expected_size => {
                return Err(DownloadError::DestinationExists);
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use reqwest::Url;

    use super::*;
    use crate::download::events::RecordingSink;
    use crate::download::request::PartRequest;
    use crate::download::test_support::FakeTokens;
    use crate::download::{
        DownloadUnit, FailureKind, FakeClock, FakeDiskSpace, FakeSleeper, FileSpec, FileStatus,
        HttpFetcher, QueueFile, RecordingStore, UnitState,
    };
    use crate::hf::RepoId;

    const MIB: u64 = 1024 * 1024;
    const REVISION: &str = "9217f5db79a29953eb74d5343926648285ec7e67";

    struct Harness {
        dir: tempfile::TempDir,
        store: Arc<RecordingStore>,
        sink: Arc<RecordingSink>,
        manager: Arc<DownloadManager>,
    }

    impl Harness {
        fn new() -> Self {
            Self::with_queue(|_| QueueFile::default())
        }

        /// Cola que ya trae una unidad en `state` con los destinos de `relative`.
        fn with_unit(relative: &[&str], state: UnitState) -> Self {
            Self::with_queue(|models_dir| {
                let dests: Vec<PathBuf> = relative
                    .iter()
                    .map(|path| repo_dir(models_dir).join(path))
                    .collect();
                QueueFile {
                    schema: 1,
                    next_id: 2,
                    units: vec![unit_with(1, &dests, state)],
                }
            })
        }

        fn with_queue(queue: impl FnOnce(&Path) -> QueueFile) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let store = Arc::new(RecordingStore::with_queue(queue(dir.path())));
            let sink = Arc::new(RecordingSink::default());
            let deps = deps(dir.path(), store.clone(), sink.clone());
            let manager = DownloadManager::new(deps).unwrap();
            Self {
                dir,
                store,
                sink,
                manager,
            }
        }

        fn dest(&self, relative: &str) -> PathBuf {
            repo_dir(self.dir.path()).join(relative)
        }

        async fn enqueue(&self, paths: &[&str]) -> Result<DownloadId, DownloadError> {
            self.manager.enqueue(request(paths, MIB)).await
        }
    }

    fn deps(
        models_dir: &Path,
        store: Arc<dyn QueuePersistence>,
        sink: Arc<RecordingSink>,
    ) -> ManagerDeps {
        ManagerDeps {
            fetcher: Arc::new(HttpFetcher::new(reqwest::Client::new(), None)),
            clock: Arc::new(FakeClock::new()),
            sleeper: Arc::new(FakeSleeper::new()),
            disk: Arc::new(FakeDiskSpace(Some(u64::MAX))),
            tokens: Arc::new(FakeTokens::new(None)),
            store,
            sink,
            models_dir: models_dir.to_path_buf(),
            hf_base_url: Url::parse("https://huggingface.co").unwrap(),
            // Sin planificador: estos tests solo miran el alta, no la ejecución.
            max_active: 0,
        }
    }

    /// Falla en el primer guardado y acepta los siguientes.
    #[derive(Default)]
    struct FailingSaveStore {
        failed_once: AtomicBool,
    }

    impl QueuePersistence for FailingSaveStore {
        fn load(&self) -> Result<QueueFile, DownloadError> {
            Ok(QueueFile::default())
        }

        fn save(&self, _queue: &QueueFile) -> Result<(), DownloadError> {
            if self.failed_once.swap(true, Ordering::SeqCst) {
                Ok(())
            } else {
                Err(DownloadError::Io(std::io::Error::other("disco lleno")))
            }
        }
    }

    fn repo_dir(models_dir: &Path) -> PathBuf {
        models_dir.join("org").join("model")
    }

    fn request(paths: &[&str], size: u64) -> UnitRequest {
        UnitRequest {
            repo: RepoId::parse("org/model").unwrap(),
            revision: REVISION.to_string(),
            parts: paths
                .iter()
                .map(|path| PartRequest {
                    relative_path: (*path).to_string(),
                    size,
                    sha256: None,
                })
                .collect(),
        }
    }

    fn unit_with(id: u64, dests: &[PathBuf], state: UnitState) -> DownloadUnit {
        DownloadUnit {
            id: DownloadId(id),
            repo_id: "org/model".to_string(),
            revision: REVISION.to_string(),
            display_name: "m".to_string(),
            files: dests
                .iter()
                .map(|dest| FileSpec {
                    relative_path: "m.gguf".to_string(),
                    url: "https://huggingface.co/x".to_string(),
                    dest: dest.clone(),
                    expected_size: MIB,
                    sha256: None,
                    status: FileStatus::Pending,
                })
                .collect(),
            state,
        }
    }

    #[tokio::test]
    async fn enqueue_crea_unidad_queued() {
        let h = Harness::new();
        let id = h.enqueue(&["m.gguf"]).await.unwrap();
        assert_eq!(id, DownloadId(1));
        let states = h.sink.states();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].state, UnitState::Queued);
        assert_eq!(h.manager.list().await.len(), 1);
        assert_eq!(h.store.save_count(), 1);
    }

    #[tokio::test]
    async fn enqueue_ids_crecientes() {
        let h = Harness::new();
        assert_eq!(h.enqueue(&["a.gguf"]).await.unwrap(), DownloadId(1));
        assert_eq!(h.enqueue(&["b.gguf"]).await.unwrap(), DownloadId(2));
        assert_eq!(h.store.last_saved().unwrap().next_id, 3);
    }

    #[tokio::test]
    async fn enqueue_dividido_una_unidad() {
        let h = Harness::new();
        h.enqueue(&[
            "m-00001-of-00003.gguf",
            "m-00002-of-00003.gguf",
            "m-00003-of-00003.gguf",
        ])
        .await
        .unwrap();
        let units = h.manager.list().await;
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].files.len(), 3);
        assert_eq!(units[0].total_bytes, 3145728);
        assert_eq!(units[0].display_name, "m");
    }

    #[tokio::test]
    async fn enqueue_split_incompleto() {
        let h = Harness::new();
        let result = h
            .enqueue(&["m-00001-of-00003.gguf", "m-00003-of-00003.gguf"])
            .await;
        assert!(matches!(result, Err(DownloadError::IncompleteSplit)));
        assert!(h.manager.list().await.is_empty());
        assert_eq!(h.store.save_count(), 0);
    }

    #[tokio::test]
    async fn enqueue_ruta_insegura() {
        let h = Harness::new();
        let result = h.enqueue(&["../x.gguf"]).await;
        assert!(matches!(result, Err(DownloadError::UnsafePath)));
        assert_eq!(h.store.save_count(), 0);
    }

    #[tokio::test]
    async fn enqueue_duplicado_rechazado() {
        let h = Harness::new();
        h.enqueue(&["m.gguf"]).await.unwrap();
        let result = h.enqueue(&["m.gguf"]).await;
        assert!(matches!(result, Err(DownloadError::AlreadyQueued)));
    }

    #[tokio::test]
    async fn enqueue_duplicado_mayusculas_rechazado() {
        let h = Harness::new();
        h.enqueue(&["Q4/M.gguf"]).await.unwrap();
        let result = h.enqueue(&["q4/m.GGUF"]).await;
        assert!(matches!(result, Err(DownloadError::AlreadyQueued)));
    }

    #[tokio::test]
    async fn enqueue_duplicado_punto_final_rechazado() {
        let h = Harness::new();
        h.enqueue(&["Q4/m.gguf"]).await.unwrap();
        let con_punto = h.enqueue(&["Q4./m.gguf"]).await;
        assert!(matches!(con_punto, Err(DownloadError::AlreadyQueued)));
        let con_espacio = h.enqueue(&["Q4 /m.gguf"]).await;
        assert!(matches!(con_espacio, Err(DownloadError::AlreadyQueued)));
    }

    #[test]
    fn dest_key_normaliza() {
        let keys: Vec<String> = ["a/B.gguf", "A/b.gguf.", "a /b.gguf "]
            .iter()
            .map(|path| dest_key(Path::new(path)))
            .collect();
        assert_eq!(keys[0], keys[1]);
        assert_eq!(keys[1], keys[2]);
    }

    #[test]
    fn dest_key_sigma_final() {
        assert_eq!(
            dest_key(Path::new("QΣ/m.gguf")),
            dest_key(Path::new("qσ/m.gguf"))
        );
        assert_eq!(
            dest_key(Path::new("mΣ-1.gguf")),
            dest_key(Path::new("mσ-1.gguf"))
        );
    }

    #[tokio::test]
    async fn enqueue_duplicado_sigma_rechazado() {
        let h = Harness::with_unit(&["QΣ/m.gguf"], UnitState::Queued);
        let result = h.enqueue(&["qσ/m.gguf"]).await;
        assert!(matches!(result, Err(DownloadError::AlreadyQueued)));
    }

    #[tokio::test]
    async fn enqueue_tras_completed_permitido() {
        let h = Harness::with_unit(&["m.gguf"], UnitState::Completed);
        assert!(h.enqueue(&["m.gguf"]).await.is_ok());
    }

    #[tokio::test]
    async fn enqueue_rechaza_destino_de_unidad_fallida() {
        let h = Harness::with_unit(
            &["Q4/m-00001-of-00002.gguf", "Q4/m-00002-of-00002.gguf"],
            UnitState::Failed(FailureKind::Network),
        );
        let result = h
            .enqueue(&["Q4/m-00001-of-00002.gguf", "Q4/m-00002-of-00002.gguf"])
            .await;
        assert!(matches!(result, Err(DownloadError::AlreadyQueued)));
        assert_eq!(h.manager.list().await.len(), 1);
        assert_eq!(h.store.save_count(), 0);
    }

    #[tokio::test]
    async fn enqueue_rechaza_destino_fallido_con_otra_grafia() {
        let h = Harness::with_unit(&["Q4/m.gguf"], UnitState::Failed(FailureKind::Network));
        let result = h.enqueue(&["q4/M.gguf"]).await;
        assert!(matches!(result, Err(DownloadError::AlreadyQueued)));
    }

    #[tokio::test]
    async fn enqueue_tras_cancelled_permitido() {
        let h = Harness::with_unit(&["m.gguf"], UnitState::Cancelled);
        assert!(h.enqueue(&["m.gguf"]).await.is_ok());
    }

    #[tokio::test]
    async fn enqueue_final_otro_tamano() {
        let h = Harness::new();
        let dest = h.dest("m.gguf");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, [0u8; 10]).unwrap();
        let result = h.manager.enqueue(request(&["m.gguf"], 20)).await;
        assert!(matches!(result, Err(DownloadError::DestinationExists)));
        assert_eq!(fs::read(&dest).unwrap(), vec![0u8; 10]);
        assert_eq!(h.store.save_count(), 0);
    }

    #[tokio::test]
    async fn enqueue_final_mismo_tamano_candidato() {
        let h = Harness::new();
        let dest = h.dest("m.gguf");
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(&dest, [0u8; 20]).unwrap();
        h.manager.enqueue(request(&["m.gguf"], 20)).await.unwrap();
        assert_eq!(h.manager.list().await.len(), 1);
        assert_eq!(fs::read(&dest).unwrap().len(), 20);
        let saved = h.store.last_saved().unwrap();
        assert_eq!(saved.units[0].files[0].status, FileStatus::Pending);
    }

    #[tokio::test]
    async fn enqueue_destino_por_repo() {
        let h = Harness::new();
        h.enqueue(&["Q8_0/m.gguf"]).await.unwrap();
        let saved = h.store.last_saved().unwrap();
        assert_eq!(
            saved.units[0].files[0].dest,
            h.dir
                .path()
                .join("org")
                .join("model")
                .join("Q8_0")
                .join("m.gguf")
        );
    }

    #[tokio::test]
    async fn snapshot_version_crece() {
        let h = Harness::new();
        h.enqueue(&["a.gguf"]).await.unwrap();
        h.enqueue(&["b.gguf"]).await.unwrap();
        let versions: Vec<u64> = h.sink.states().iter().map(|s| s.version).collect();
        assert_eq!(versions.len(), 2);
        assert!(versions[0] < versions[1]);
        assert_eq!(h.manager.list().await[0].version, versions[1]);
    }

    #[tokio::test]
    async fn enqueue_revierte_si_falla_el_guardado() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(RecordingSink::default());
        let store = Arc::new(FailingSaveStore::default());
        let manager = DownloadManager::new(deps(dir.path(), store, sink.clone())).unwrap();
        let result = manager.enqueue(request(&["m.gguf"], MIB)).await;
        assert!(matches!(result, Err(DownloadError::Io(_))));
        assert!(manager.list().await.is_empty());
        assert!(sink.states().is_empty());

        let id = manager.enqueue(request(&["m.gguf"], MIB)).await.unwrap();
        assert_eq!(id, DownloadId(1));
        let states = sink.states();
        assert_eq!(states.len(), 1);
        assert_eq!(states[0].version, 1);
    }

    #[tokio::test]
    async fn enqueue_url_usa_revision() {
        let h = Harness::new();
        h.enqueue(&["m.gguf"]).await.unwrap();
        let saved = h.store.last_saved().unwrap();
        assert!(
            saved.units[0].files[0]
                .url
                .contains(&format!("/resolve/{REVISION}/"))
        );
    }
}
