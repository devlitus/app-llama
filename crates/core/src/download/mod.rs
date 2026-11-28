//! Cola de descargas, reanudación y progreso (fase 3).

mod clock;
mod error;
mod http;
mod model;
mod paths;
mod progress;
mod split;

pub use clock::{Clock, SystemClock};
#[cfg(test)]
pub use clock::{FakeClock, SteppingClock};
pub use error::DownloadError;
pub use http::{
    ContentRange, InvalidContentRange, ResponseAction, ResponseInfo, classify, parse_content_range,
    range_header,
};
pub use model::{DownloadId, DownloadUnit, FailureKind, FileSpec, FileStatus, UnitState};
pub use paths::{part_path, safe_join};
pub use progress::{Progress, ProgressReporter, ProgressTracker, Throttle};
pub use split::{display_name, validate_parts};
