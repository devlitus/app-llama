//! Cliente de la API de Hugging Face (fase 2).
pub mod budget_cache;
pub mod client;
pub mod error;
pub mod files;
pub mod fit;
pub mod hint;
pub mod memory;
pub mod model;
pub mod quant;
pub mod service;
pub mod token;
pub mod url;

pub use budget_cache::BudgetCache;
pub use client::{HfClient, SearchPage};
pub use error::HfError;
pub use files::{DownloadRequest, GgufPart, GgufVariant, group_gguf_files};
pub use fit::{Fit, MemoryBudget, SAFETY_MARGIN_PERCENT, fit, recommended};
pub use hint::{FitHint, TYPICAL_QUANT_MILLIBYTES_PER_PARAM, fit_hint, typical_quant_bytes};
pub use memory::{MemoryEstimate, estimate_memory, estimation_ctx};
pub use model::{
    Gated, GgufInfo, LfsInfo, ModelInfo, ModelSummary, TreeEntry, TreeEntryKind, WhoAmI,
};
pub use quant::extract_quant;
pub use service::{HfService, ModelDetail, SearchItem, SearchResults, TokenStatus, VariantView};
pub use token::HfToken;
pub use url::{
    RepoId, SearchParams, SortBy, model_info_url, parse_next_link, search_url, tree_url, whoami_url,
};
