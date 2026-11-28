//! Eventos que el gestor emite hacia quien lo use (en la app, la UI).
use serde::{Deserialize, Serialize};

use super::model::{DownloadId, DownloadUnit, FileStatus, UnitState};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSnapshot {
    pub relative_path: String,
    pub size: u64,
    pub status: FileStatus,
}

/// Estado de una unidad en un instante. `version` crece con cada cambio de
/// estado del gestor, para que quien lo recibe descarte los atrasados.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadSnapshot {
    pub id: DownloadId,
    pub version: u64,
    pub repo_id: String,
    pub revision: String,
    pub display_name: String,
    pub state: UnitState,
    pub total_bytes: u64,
    pub files: Vec<FileSnapshot>,
}

impl DownloadSnapshot {
    pub fn of(unit: &DownloadUnit, version: u64) -> Self {
        Self {
            id: unit.id,
            version,
            repo_id: unit.repo_id.clone(),
            revision: unit.revision.clone(),
            display_name: unit.display_name.clone(),
            state: unit.state.clone(),
            total_bytes: unit.files.iter().map(|file| file.expected_size).sum(),
            files: unit
                .files
                .iter()
                .map(|file| FileSnapshot {
                    relative_path: file.relative_path.clone(),
                    size: file.expected_size,
                    status: file.status,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProgressPhase {
    Download,
    Verify,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub id: DownloadId,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub bytes_per_sec: u64,
    pub eta_secs: Option<u64>,
    pub phase: ProgressPhase,
}

pub trait EventSink: Send + Sync {
    fn state(&self, snapshot: &DownloadSnapshot);
    fn progress(&self, progress: &DownloadProgress);
}

/// Sumidero de los tests: guarda los eventos en el orden en que llegan.
#[cfg(test)]
#[derive(Default)]
pub struct RecordingSink {
    states: std::sync::Mutex<Vec<DownloadSnapshot>>,
    progress: std::sync::Mutex<Vec<DownloadProgress>>,
}

#[cfg(test)]
impl RecordingSink {
    pub fn states(&self) -> Vec<DownloadSnapshot> {
        self.states.lock().expect("registro de pruebas").clone()
    }

    pub fn progress(&self) -> Vec<DownloadProgress> {
        self.progress.lock().expect("registro de pruebas").clone()
    }
}

#[cfg(test)]
impl EventSink for RecordingSink {
    fn state(&self, snapshot: &DownloadSnapshot) {
        self.states
            .lock()
            .expect("registro de pruebas")
            .push(snapshot.clone());
    }

    fn progress(&self, progress: &DownloadProgress) {
        self.progress
            .lock()
            .expect("registro de pruebas")
            .push(progress.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(downloaded_bytes: u64) -> DownloadProgress {
        DownloadProgress {
            id: DownloadId(1),
            downloaded_bytes,
            total_bytes: 10,
            bytes_per_sec: 0,
            eta_secs: None,
            phase: ProgressPhase::Download,
        }
    }

    #[test]
    fn recording_sink_conserva_el_orden_del_progreso() {
        let sink = RecordingSink::default();
        EventSink::progress(&sink, &progress(1));
        EventSink::progress(&sink, &progress(2));
        assert_eq!(sink.progress(), vec![progress(1), progress(2)]);
    }
}
