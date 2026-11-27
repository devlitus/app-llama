//! Cliente de la API de Hugging Face (fase 2).
pub mod error;
pub mod files;
pub mod fit;
pub mod memory;
pub mod model;
pub mod quant;

pub use error::HfError;
pub use files::{DownloadRequest, GgufPart, GgufVariant, group_gguf_files};
pub use fit::{Fit, MemoryBudget, SAFETY_MARGIN_PERCENT, fit, recommended};
pub use memory::{MemoryEstimate, estimate_memory, estimation_ctx};
pub use model::{
    Gated, GgufInfo, LfsInfo, ModelInfo, ModelSummary, TreeEntry, TreeEntryKind, WhoAmI,
};
pub use quant::extract_quant;
