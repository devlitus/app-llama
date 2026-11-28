//! Cola de descargas, reanudación y progreso (fase 3).

mod auth;
mod clock;
mod disk;
mod error;
mod http;
mod model;
mod paths;
mod progress;
mod retry;
mod split;
mod store;
#[cfg(test)]
mod test_support;
mod transfer;
mod verify;

pub use auth::{TokenProvider, auth_header_for, auth_source};
pub use clock::{Clock, SystemClock};
#[cfg(test)]
pub use clock::{FakeClock, SteppingClock};
#[cfg(test)]
pub use disk::FakeDiskSpace;
pub use disk::{DiskSpace, Fs4DiskSpace, SPACE_MARGIN, check_space, required_bytes};
pub use error::DownloadError;
pub use http::{
    ContentRange, InvalidContentRange, ResponseAction, ResponseInfo, classify, parse_content_range,
    range_header,
};
pub use model::{DownloadId, DownloadUnit, FailureKind, FileSpec, FileStatus, UnitState};
pub use paths::{part_path, safe_join};
pub use progress::{Progress, ProgressReporter, ProgressTracker, Throttle};
#[cfg(test)]
pub use retry::FakeSleeper;
pub use retry::{RetryPolicy, Sleeper, TokioSleeper, transfer_with_retry};
pub use split::{display_name, validate_parts};
#[cfg(test)]
pub use store::RecordingStore;
pub use store::{JsonQueueStore, QueueFile, QueuePersistence};
pub use transfer::{HttpFetcher, TransferOutcome, network_failure, reset_part, transfer_once};
pub use verify::{finalize_unit, sha256_file, verify_file};
