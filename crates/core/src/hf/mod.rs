//! Cliente de la API de Hugging Face (fase 2).
pub mod error;
pub mod model;

pub use error::HfError;
pub use model::{
    Gated, GgufInfo, LfsInfo, ModelInfo, ModelSummary, TreeEntry, TreeEntryKind, WhoAmI,
};
