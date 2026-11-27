//! Detección de hardware e instalación o selección de llama.cpp (fase 1).
mod release;

pub use release::{Asset, Release, build_number, latest_build_release, parse_releases};

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("no se pudo interpretar la lista de releases: {0}")]
    ReleaseParse(#[source] serde_json::Error),
    #[error("ninguna release de llama.cpp tiene un asset compatible con esta máquina")]
    NoCompatibleRelease,
}
