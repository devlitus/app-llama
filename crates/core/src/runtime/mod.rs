//! Detección de hardware e instalación o selección de llama.cpp (fase 1).
mod asset;
mod hardware;
mod process;
mod release;
mod select;
#[cfg(test)]
mod test_support;

pub use asset::{
    Arch, ArchiveFormat, AssetDescriptor, AssetKind, Backend, CpuIsa, Os, classify_asset,
};
pub use hardware::{CudaVersion, GpuInfo, HardwareInfo, NativeProbe, SystemProbe, detect};
pub use release::{Asset, Release, build_number, latest_build_release, parse_releases};
pub use select::{
    BackendPreference, InstallPlan, available_backends, is_compatible, select_install,
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
}
