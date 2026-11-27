//! Detección de hardware e instalación o selección de llama.cpp (fase 1).
mod asset;
mod hardware;
mod process;
mod release;
#[cfg(test)]
mod test_support;

pub use asset::{
    Arch, ArchiveFormat, AssetDescriptor, AssetKind, Backend, CpuIsa, Os, classify_asset,
};
pub use hardware::{CudaVersion, GpuInfo, HardwareInfo, NativeProbe, SystemProbe, detect};
pub use release::{Asset, Release, build_number, latest_build_release, parse_releases};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    // El error de serde viaja como `source`; no se repite en el mensaje.
    #[error("no se pudo interpretar la lista de releases")]
    ReleaseParse(#[source] serde_json::Error),
    #[error("ninguna release de llama.cpp tiene un asset compatible con esta máquina")]
    NoCompatibleRelease,
}
