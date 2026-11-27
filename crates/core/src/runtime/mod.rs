//! Detección de hardware e instalación o selección de llama.cpp (fase 1).
mod asset;
mod extract;
mod fetch;
mod github;
mod hardware;
mod install;
mod manager;
mod process;
mod release;
mod select;
mod state;
#[cfg(test)]
mod test_support;
mod verify;

pub use asset::{
    Arch, ArchiveFormat, AssetDescriptor, AssetKind, Backend, CpuIsa, Os, classify_asset,
};
pub use extract::{extract_archive, find_server_binary};
pub use fetch::{ArtifactFetcher, CancelToken, FetchRequest, SimpleHttpFetcher};
pub use github::{GitHubReleaseSource, ReleaseSource};
pub use hardware::{CudaVersion, GpuInfo, HardwareInfo, NativeProbe, SystemProbe, detect};
pub use install::{ProgressEvent, ProgressPhase};
pub use manager::{DirRemover, RuntimeDeps, RuntimeManager, StdDirRemover, UpdateInfo};
pub use release::{Asset, Release, build_number, latest_build_release, parse_releases};
pub use select::{
    BackendPreference, InstallPlan, available_backends, is_compatible, select_install,
};
pub use state::{ActiveRuntime, RuntimeSource, RuntimeState, RuntimeStatus, StateStore, status_of};
pub use verify::{
    CommandOutput, CommandRunner, RuntimeVersion, TokioCommandRunner, parse_version,
    remove_quarantine, verify_server,
};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    // El error de serde viaja como `source`; no se repite en el mensaje.
    #[error("no se pudo interpretar la lista de releases")]
    ReleaseParse(#[source] serde_json::Error),
    #[error("ninguna release de llama.cpp tiene un asset compatible con esta máquina")]
    NoCompatibleRelease,
    #[error("ningún asset de esta release es compatible con esta máquina")]
    NoCompatibleAsset,
    #[error("el backend elegido no está disponible en esta máquina")]
    BackendUnavailable,
    #[error("fallo de red")]
    Http(#[from] reqwest::Error),
    #[error("fallo de entrada/salida")]
    Io(#[from] std::io::Error),
    #[error("el servidor respondió con el estado HTTP {0}")]
    HttpStatus(u16),
    #[error("límite de peticiones de la API de GitHub alcanzado")]
    RateLimited { reset_epoch: Option<u64> },
    #[error("tamaño descargado incorrecto: esperado {expected} bytes, recibido {actual}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("el SHA256 de la descarga no coincide con el publicado")]
    IntegrityMismatch,
    #[error("operación cancelada")]
    Cancelled,
    #[error("el archivo contiene una ruta que sale del directorio de destino: {0}")]
    UnsafeArchivePath(String),
    #[error("no se pudo extraer el archivo: {0}")]
    Extract(String),
    #[error("no se encontró llama-server en el archivo extraído")]
    ServerBinaryNotFound,
    #[error("llama-server no se pudo verificar: {stderr_tail}")]
    VerificationFailed { stderr_tail: String },
    #[error("no se pudo quitar la cuarentena de macOS: {0}")]
    QuarantineFailed(String),
    #[error("ya hay una instalación en curso")]
    AlreadyInstalling,
    #[error("el binario debe llamarse llama-server o llama-server.exe")]
    InvalidExternalBinary,
}
