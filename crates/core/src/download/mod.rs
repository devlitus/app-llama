//! Cola de descargas, reanudación y progreso (fase 3).

mod error;
mod model;
mod paths;
mod split;

pub use error::DownloadError;
pub use model::{DownloadId, DownloadUnit, FailureKind, FileSpec, FileStatus, UnitState};
pub use paths::{part_path, safe_join};
pub use split::{display_name, validate_parts};
