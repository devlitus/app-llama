//! Cliente de la API de Hugging Face (fase 2).
pub mod budget_cache;
pub mod error;
pub mod files;
pub mod fit;
pub mod hint;
pub mod memory;
pub mod model;
pub mod quant;

pub use budget_cache::BudgetCache;
pub use error::HfError;
pub use files::{DownloadRequest, GgufPart, GgufVariant, group_gguf_files};
pub use fit::{Fit, MemoryBudget, SAFETY_MARGIN_PERCENT, fit, recommended};
pub use hint::{FitHint, TYPICAL_QUANT_MILLIBYTES_PER_PARAM, fit_hint, typical_quant_bytes};
pub use memory::{MemoryEstimate, estimate_memory, estimation_ctx};
pub use model::{
    Gated, GgufInfo, LfsInfo, ModelInfo, ModelSummary, TreeEntry, TreeEntryKind, WhoAmI,
};
pub use quant::extract_quant;
