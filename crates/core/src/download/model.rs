use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DownloadId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileStatus {
    Pending,
    Verified,
    Preexisting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileSpec {
    pub relative_path: String,
    pub url: String,
    pub dest: PathBuf,
    pub expected_size: u64,
    pub sha256: Option<String>,
    pub status: FileStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
pub enum FailureKind {
    #[error("error de red")]
    Network,
    #[error("hace falta un token de Hugging Face válido")]
    Unauthorized,
    #[error("el modelo requiere aceptar su licencia o no hay acceso")]
    Gated,
    #[error("fichero no encontrado en Hugging Face")]
    NotFound,
    #[error("el SHA256 del fichero no coincide")]
    ChecksumMismatch,
    #[error("tamaño inesperado: esperado {expected} bytes, recibido {actual}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("espacio en disco insuficiente: hacen falta {needed} bytes y hay {available}")]
    InsufficientSpace { needed: u64, available: u64 },
    #[error("el servidor respondió con un rango incoherente")]
    UnexpectedContentRange,
    #[error("el fichero de destino ya existe con otro contenido")]
    DestinationExists,
    #[error("error de E/S: {message}")]
    Io { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitState {
    Queued,
    Downloading,
    Verifying,
    Paused,
    Completed,
    Failed(FailureKind),
    Cancelled,
}

impl UnitState {
    /// Orden de usuario (pausar, reanudar, cancelar, reintentar) o transición
    /// interna del gestor permitida desde `self` hacia `to`.
    pub fn can_transition(&self, to: &UnitState) -> bool {
        use UnitState::*;
        matches!(
            (self, to),
            (Queued, Downloading | Paused | Cancelled | Failed(_))
                | (
                    Downloading,
                    Verifying | Paused | Cancelled | Failed(_) | Queued
                )
                | (Verifying, Completed | Cancelled | Failed(_))
                | (Paused, Queued | Cancelled)
                | (Failed(_), Queued)
        )
    }

    /// «Quitar» solo saca de la lista las unidades que ya no van a ejecutarse.
    pub fn can_remove(&self) -> bool {
        matches!(
            self,
            UnitState::Completed | UnitState::Failed(_) | UnitState::Cancelled
        )
    }

    /// Una unidad así reserva sus destinos: no se puede encolar otra sobre ellos.
    /// Solo `Completed` y `Cancelled` los liberan.
    pub fn blocks_destination(&self) -> bool {
        !matches!(self, UnitState::Completed | UnitState::Cancelled)
    }
}

/// Una variante del modelo, con todas sus partes, que se descarga como una unidad.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadUnit {
    pub id: DownloadId,
    pub repo_id: String,
    pub revision: String,
    pub display_name: String,
    pub files: Vec<FileSpec>,
    pub state: UnitState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transicion_queued_a_downloading() {
        assert!(UnitState::Queued.can_transition(&UnitState::Downloading));
    }

    #[test]
    fn transicion_completed_es_terminal() {
        assert!(!UnitState::Completed.can_transition(&UnitState::Queued));
    }

    #[test]
    fn transicion_failed_a_queued() {
        assert!(UnitState::Failed(FailureKind::Network).can_transition(&UnitState::Queued));
    }

    #[test]
    fn transicion_verifying_no_pausa() {
        assert!(!UnitState::Verifying.can_transition(&UnitState::Paused));
    }

    #[test]
    fn transicion_paused_a_cancelled() {
        assert!(UnitState::Paused.can_transition(&UnitState::Cancelled));
    }

    #[test]
    fn transicion_queued_a_failed() {
        let failure = FailureKind::InsufficientSpace {
            needed: 2,
            available: 1,
        };
        assert!(UnitState::Queued.can_transition(&UnitState::Failed(failure)));
    }

    #[test]
    fn blocks_destination_salvo_completed_y_cancelled() {
        assert!(UnitState::Queued.blocks_destination());
        assert!(UnitState::Downloading.blocks_destination());
        assert!(UnitState::Verifying.blocks_destination());
        assert!(UnitState::Paused.blocks_destination());
        assert!(UnitState::Failed(FailureKind::Network).blocks_destination());
        assert!(!UnitState::Completed.blocks_destination());
        assert!(!UnitState::Cancelled.blocks_destination());
    }

    #[test]
    fn solo_las_terminales_se_pueden_quitar() {
        assert!(UnitState::Completed.can_remove());
        assert!(UnitState::Cancelled.can_remove());
        assert!(UnitState::Failed(FailureKind::Network).can_remove());
        assert!(!UnitState::Queued.can_remove());
        assert!(!UnitState::Downloading.can_remove());
        assert!(!UnitState::Verifying.can_remove());
        assert!(!UnitState::Paused.can_remove());
    }

    #[test]
    fn unit_state_serde_ida_vuelta() {
        let state = UnitState::Failed(FailureKind::InsufficientSpace {
            needed: 65498251264,
            available: 1,
        });
        let json = serde_json::to_string(&state).unwrap();
        assert_eq!(
            json,
            r#"{"Failed":{"InsufficientSpace":{"needed":65498251264,"available":1}}}"#
        );
        assert_eq!(serde_json::from_str::<UnitState>(&json).unwrap(), state);
    }
}
