//! Persistencia de la cola de descargas en `downloads.json`.
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::error::DownloadError;
use super::model::{DownloadUnit, UnitState};

const QUEUE_FILE: &str = "downloads.json";
const TEMP_FILE: &str = "downloads.json.tmp";
const BACKUP_FILE: &str = "downloads.json.bak";
const SCHEMA_VERSION: u32 = 1;
const FIRST_ID: u64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueFile {
    pub schema: u32,
    pub next_id: u64,
    pub units: Vec<DownloadUnit>,
}

impl Default for QueueFile {
    fn default() -> Self {
        Self {
            schema: SCHEMA_VERSION,
            next_id: FIRST_ID,
            units: Vec::new(),
        }
    }
}

pub trait QueuePersistence: Send + Sync {
    fn load(&self) -> Result<QueueFile, DownloadError>;
    fn save(&self, queue: &QueueFile) -> Result<(), DownloadError>;
}

/// No es seguro para escrituras concurrentes: el fichero temporal tiene nombre fijo.
pub struct JsonQueueStore {
    dir: PathBuf,
}

impl JsonQueueStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }
}

impl JsonQueueStore {
    // La copia es un auxilio de diagnóstico: si falla, arrancar limpio sigue siendo lo correcto.
    fn back_up(&self, path: &Path) {
        let _ = fs::copy(path, self.dir.join(BACKUP_FILE));
    }
}

/// Nada arranca solo tras un reinicio: lo que estaba en marcha o en espera queda en pausa.
fn restore_as_paused(queue: &mut QueueFile) {
    for unit in &mut queue.units {
        if matches!(
            unit.state,
            UnitState::Downloading | UnitState::Verifying | UnitState::Queued
        ) {
            unit.state = UnitState::Paused;
        }
    }
}

impl QueuePersistence for JsonQueueStore {
    /// Un fichero ausente equivale a la cola vacía. Uno ilegible (no UTF-8, no parseable o de
    /// otro `schema`) también, pero se conserva como `.bak`. Cualquier otro fallo de lectura se
    /// devuelve sin tocar nada: tratarlo como cola vacía dejaría que un `save` posterior la destruya.
    fn load(&self) -> Result<QueueFile, DownloadError> {
        let path = self.dir.join(QUEUE_FILE);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(QueueFile::default()),
            Err(error) => return Err(error.into()),
        };
        match serde_json::from_slice::<QueueFile>(&bytes) {
            Ok(mut queue) if queue.schema == SCHEMA_VERSION => {
                restore_as_paused(&mut queue);
                Ok(queue)
            }
            _ => {
                self.back_up(&path);
                Ok(QueueFile::default())
            }
        }
    }

    fn save(&self, queue: &QueueFile) -> Result<(), DownloadError> {
        fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_string_pretty(queue).map_err(std::io::Error::from)?;
        let temp = self.dir.join(TEMP_FILE);
        fs::write(&temp, json)?;
        fs::rename(&temp, self.dir.join(QUEUE_FILE))?;
        Ok(())
    }
}

/// Almacén en memoria para los tests del gestor: guarda cada `save` y los cuenta.
#[cfg(test)]
#[derive(Default)]
pub struct RecordingStore {
    initial: QueueFile,
    saved: std::sync::Mutex<Vec<QueueFile>>,
}

#[cfg(test)]
impl RecordingStore {
    pub fn with_queue(initial: QueueFile) -> Self {
        Self {
            initial,
            saved: Default::default(),
        }
    }

    pub fn save_count(&self) -> usize {
        self.saved.lock().expect("registro de pruebas").len()
    }

    pub fn last_saved(&self) -> Option<QueueFile> {
        self.saved
            .lock()
            .expect("registro de pruebas")
            .last()
            .cloned()
    }
}

#[cfg(test)]
impl QueuePersistence for RecordingStore {
    fn load(&self) -> Result<QueueFile, DownloadError> {
        Ok(self.initial.clone())
    }

    fn save(&self, queue: &QueueFile) -> Result<(), DownloadError> {
        self.saved
            .lock()
            .expect("registro de pruebas")
            .push(queue.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::model::{DownloadId, FailureKind, FileSpec, FileStatus};
    use crate::download::test_support::hf_token;

    fn unit(id: u64, state: UnitState) -> DownloadUnit {
        DownloadUnit {
            id: DownloadId(id),
            repo_id: "owner/name".to_string(),
            revision: "main".to_string(),
            display_name: "m-Q4".to_string(),
            files: vec![FileSpec {
                relative_path: "m.gguf".to_string(),
                url: "https://huggingface.co/owner/name/resolve/main/m.gguf".to_string(),
                dest: PathBuf::from("models")
                    .join("owner")
                    .join("name")
                    .join("m.gguf"),
                expected_size: 10,
                sha256: Some("ab".repeat(32)),
                status: FileStatus::Pending,
            }],
            state,
        }
    }

    fn queue(next_id: u64, states: Vec<UnitState>) -> QueueFile {
        let units = states
            .into_iter()
            .zip(1..)
            .map(|(state, id)| unit(id, state))
            .collect();
        QueueFile {
            schema: 1,
            next_id,
            units,
        }
    }

    fn states(queue: &QueueFile) -> Vec<UnitState> {
        queue.units.iter().map(|u| u.state.clone()).collect()
    }

    #[test]
    fn guardar_y_cargar_ida_vuelta() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonQueueStore::new(dir.path());
        let saved = queue(
            7,
            vec![
                UnitState::Completed,
                UnitState::Failed(FailureKind::Network),
            ],
        );
        store.save(&saved).unwrap();
        assert_eq!(store.load().unwrap(), saved);
    }

    #[test]
    fn carga_normaliza_activos_a_paused() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonQueueStore::new(dir.path());
        store
            .save(&queue(
                5,
                vec![
                    UnitState::Downloading,
                    UnitState::Queued,
                    UnitState::Verifying,
                    UnitState::Completed,
                ],
            ))
            .unwrap();
        assert_eq!(
            states(&store.load().unwrap()),
            vec![
                UnitState::Paused,
                UnitState::Paused,
                UnitState::Paused,
                UnitState::Completed
            ]
        );
    }

    #[test]
    fn fichero_inexistente_cola_vacia() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = JsonQueueStore::new(dir.path()).load().unwrap();
        assert_eq!(loaded, QueueFile::default());
        assert_eq!(loaded.next_id, 1);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    fn assert_apartado(content: &[u8]) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("downloads.json"), content).unwrap();
        let loaded = JsonQueueStore::new(dir.path()).load().unwrap();
        assert_eq!(loaded, QueueFile::default());
        let backup = std::fs::read(dir.path().join("downloads.json.bak")).unwrap();
        assert_eq!(backup, content);
    }

    #[test]
    fn json_corrupto_se_aparta() {
        assert_apartado(b"{no json");
    }

    #[test]
    fn bytes_no_utf8_se_apartan() {
        assert_apartado(&[0xff, 0xfe, 0x00]);
    }

    #[test]
    fn schema_desconocido_se_aparta() {
        assert_apartado(br#"{"schema":99,"next_id":3,"units":[]}"#);
    }

    #[test]
    fn error_de_lectura_no_es_cola_vacia() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("downloads.json")).unwrap();
        let result = JsonQueueStore::new(dir.path()).load();
        assert!(matches!(result, Err(DownloadError::Io(_))));
        assert!(!dir.path().join("downloads.json.bak").exists());
    }

    #[test]
    fn guardado_atomico_no_deja_tmp() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonQueueStore::new(dir.path().join("a").join("b"));
        store.save(&queue(2, vec![UnitState::Paused])).unwrap();
        store.save(&queue(3, vec![UnitState::Paused])).unwrap();
        let target = dir.path().join("a").join("b");
        assert!(!target.join("downloads.json.tmp").exists());
        let json = std::fs::read_to_string(target.join("downloads.json")).unwrap();
        assert!(serde_json::from_str::<QueueFile>(&json).is_ok());
        assert_eq!(store.load().unwrap().next_id, 3);
    }

    #[test]
    fn almacen_en_memoria_cuenta_los_guardados() {
        let store = RecordingStore::with_queue(queue(4, vec![UnitState::Paused]));
        assert_eq!(store.load().unwrap().next_id, 4);
        store.save(&queue(5, vec![])).unwrap();
        assert_eq!(store.save_count(), 1);
        assert_eq!(store.last_saved().unwrap().next_id, 5);
    }

    #[test]
    fn no_persiste_token_ni_url_cdn() {
        let dir = tempfile::tempdir().unwrap();
        let token = hf_token('A');
        let store = JsonQueueStore::new(dir.path());
        store.save(&queue(2, vec![UnitState::Downloading])).unwrap();
        let json = std::fs::read_to_string(dir.path().join("downloads.json")).unwrap();
        assert!(!json.contains(token.expose()));
        assert!(!json.contains("cas-bridge"));
        assert!(!json.contains("Authorization"));
    }
}
