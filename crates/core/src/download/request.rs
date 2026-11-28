//! Petición de alta de una unidad de descarga.
use crate::hf::RepoId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartRequest {
    pub relative_path: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitRequest {
    pub repo: RepoId,
    pub revision: String,
    pub parts: Vec<PartRequest>,
}
