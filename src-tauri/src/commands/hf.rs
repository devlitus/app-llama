//! Comandos Tauri del explorador de Hugging Face.
use std::error::Error;
use std::time::Duration;

use app_llama_core::hf::token::KeyringStore;
use app_llama_core::hf::{
    BudgetCache, HfError, HfService, MemoryBudget, ModelDetail, SearchItem, SearchParams,
    SearchResults, SortBy, TokenStatus, WhoAmI,
};
use reqwest::Url;
use serde::Serialize;
use tauri::State;

use super::error::ErrorPayload;
use super::runtime::RuntimeAppState;

const HF_BASE_URL: &str = "https://huggingface.co";
const USER_AGENT: &str = concat!("app-llama/", env!("CARGO_PKG_VERSION"));
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const SEARCH_PAGE_SIZE: u32 = 20;

/// `reqwest::Url` no serializa, así que la página lleva el enlace siguiente como texto.
#[derive(Debug, Serialize)]
pub struct SearchPageDto {
    pub items: Vec<SearchItem>,
    pub next: Option<String>,
}

impl From<SearchResults> for SearchPageDto {
    fn from(results: SearchResults) -> Self {
        Self {
            items: results.items,
            next: results.next.map(String::from),
        }
    }
}

pub struct HfAppState {
    service: HfService,
    budget: BudgetCache,
}

impl HfAppState {
    /// Si la detección del hardware falla, se sigue sin etiquetas de encaje.
    async fn budget(&self, runtime: &RuntimeAppState) -> MemoryBudget {
        self.budget.get_or_detect(|| runtime.hardware()).await
    }
}

/// Estado con las implementaciones reales. No toca la red al arrancar.
pub fn production_state() -> Result<HfAppState, Box<dyn Error>> {
    let http = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()?;
    let base_url = Url::parse(HF_BASE_URL)?;
    Ok(HfAppState {
        service: HfService::new(http, base_url, Box::new(KeyringStore)),
        budget: BudgetCache::new(),
    })
}

pub fn hf_error_to_payload(error: &HfError) -> ErrorPayload {
    ErrorPayload {
        code: hf_error_code(error),
        message: hf_error_message(error),
    }
}

impl From<HfError> for ErrorPayload {
    fn from(error: HfError) -> Self {
        hf_error_to_payload(&error)
    }
}

fn hf_error_code(error: &HfError) -> &'static str {
    match error {
        HfError::Network => "Network",
        HfError::Unauthorized => "Unauthorized",
        HfError::Forbidden => "Forbidden",
        HfError::NotFound => "NotFound",
        HfError::RateLimited { .. } => "RateLimited",
        HfError::Status(_) => "Status",
        HfError::Parse(_) => "Parse",
        HfError::InvalidRepoId => "InvalidRepoId",
        HfError::InvalidToken => "InvalidToken",
        HfError::TooManyPages => "TooManyPages",
        HfError::ForeignUrl => "ForeignUrl",
        HfError::Secret(_) => "Secret",
    }
}

/// Nunca incluye el detalle de `Secret` ni de `Parse`: podrían arrastrar datos del usuario.
fn hf_error_message(error: &HfError) -> String {
    match error {
        HfError::Network => "Sin conexión con Hugging Face".to_owned(),
        HfError::Unauthorized | HfError::Forbidden => "Acceso denegado: revisa tu token".to_owned(),
        HfError::NotFound => "Modelo no encontrado".to_owned(),
        HfError::RateLimited {
            retry_after_secs: Some(secs),
        } => format!("Hugging Face limita las peticiones; reintenta en {secs} s"),
        HfError::RateLimited {
            retry_after_secs: None,
        } => "Hugging Face limita las peticiones; reintenta más tarde".to_owned(),
        HfError::Status(status) => format!("Hugging Face respondió con el estado HTTP {status}"),
        HfError::Parse(_) => "Respuesta inesperada de Hugging Face".to_owned(),
        HfError::InvalidRepoId => "Identificador de modelo no válido".to_owned(),
        HfError::InvalidToken => "El token no tiene un formato válido".to_owned(),
        HfError::TooManyPages => "Se superó el límite de páginas de resultados".to_owned(),
        HfError::ForeignUrl => "El enlace de paginación no es válido".to_owned(),
        HfError::Secret(_) => "No se pudo acceder al almacén seguro de credenciales".to_owned(),
    }
}

#[tauri::command]
pub async fn hf_search(
    hf: State<'_, HfAppState>,
    runtime: State<'_, RuntimeAppState>,
    query: String,
    sort: SortBy,
) -> Result<SearchPageDto, ErrorPayload> {
    let budget = hf.budget(&runtime).await;
    let params = SearchParams {
        query,
        sort,
        limit: SEARCH_PAGE_SIZE,
    };
    Ok(hf.service.search(&params, &budget).await?.into())
}

#[tauri::command]
pub async fn hf_search_next(
    hf: State<'_, HfAppState>,
    runtime: State<'_, RuntimeAppState>,
    next_url: String,
) -> Result<SearchPageDto, ErrorPayload> {
    let next = Url::parse(&next_url).map_err(|_| HfError::ForeignUrl)?;
    let budget = hf.budget(&runtime).await;
    Ok(hf.service.search_next(&next, &budget).await?.into())
}

#[tauri::command]
pub async fn hf_model_detail(
    hf: State<'_, HfAppState>,
    runtime: State<'_, RuntimeAppState>,
    repo_id: String,
) -> Result<ModelDetail, ErrorPayload> {
    let budget = hf.budget(&runtime).await;
    Ok(hf.service.model_detail(&repo_id, &budget).await?)
}

#[tauri::command]
pub async fn hf_token_status(hf: State<'_, HfAppState>) -> Result<TokenStatus, ErrorPayload> {
    Ok(hf.service.token_status().await?)
}

#[tauri::command]
pub async fn hf_token_save(
    hf: State<'_, HfAppState>,
    token: String,
) -> Result<WhoAmI, ErrorPayload> {
    Ok(hf.service.save_token(&token).await?)
}

#[tauri::command]
pub async fn hf_token_delete(hf: State<'_, HfAppState>) -> Result<(), ErrorPayload> {
    Ok(hf.service.delete_token()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_llama_core::hf::{
        Fit, FitHint, Gated, GgufInfo, GgufPart, GgufVariant, MemoryEstimate, ModelInfo,
        ModelSummary, VariantView,
    };
    use serde_json::json;

    fn payload(error: HfError) -> ErrorPayload {
        hf_error_to_payload(&error)
    }

    #[test]
    fn error_payload_hf_rate_limited() {
        let payload = payload(HfError::RateLimited {
            retry_after_secs: Some(30),
        });
        assert_eq!(payload.code, "RateLimited");
        assert!(payload.message.contains("reintenta en 30 s"));
    }

    #[test]
    fn error_payload_hf_rate_limited_without_secs() {
        let payload = payload(HfError::RateLimited {
            retry_after_secs: None,
        });
        assert!(payload.message.contains("reintenta más tarde"));
    }

    #[test]
    fn error_payload_hf_not_found() {
        let payload = payload(HfError::NotFound);
        assert_eq!(payload.code, "NotFound");
        assert!(payload.message.contains("Modelo no encontrado"));
    }

    #[test]
    fn error_payload_hf_unauthorized() {
        let payload = payload(HfError::Unauthorized);
        assert_eq!(payload.code, "Unauthorized");
        assert!(payload.message.contains("Acceso denegado: revisa tu token"));
    }

    #[test]
    fn error_payload_hf_forbidden() {
        let payload = payload(HfError::Forbidden);
        assert_eq!(payload.code, "Forbidden");
        assert!(payload.message.contains("Acceso denegado: revisa tu token"));
    }

    #[test]
    fn error_payload_hf_parse() {
        let payload = payload(HfError::Parse("x".to_owned()));
        assert!(
            payload
                .message
                .contains("Respuesta inesperada de Hugging Face")
        );
    }

    #[test]
    fn error_payload_hf_network() {
        assert!(
            payload(HfError::Network)
                .message
                .contains("Sin conexión con Hugging Face")
        );
    }

    #[test]
    fn error_payload_hf_codes() {
        let cases = [
            (HfError::Network, "Network"),
            (HfError::Unauthorized, "Unauthorized"),
            (HfError::Forbidden, "Forbidden"),
            (HfError::NotFound, "NotFound"),
            (
                HfError::RateLimited {
                    retry_after_secs: None,
                },
                "RateLimited",
            ),
            (HfError::Status(500), "Status"),
            (HfError::Parse("x".to_owned()), "Parse"),
            (HfError::InvalidRepoId, "InvalidRepoId"),
            (HfError::InvalidToken, "InvalidToken"),
            (HfError::TooManyPages, "TooManyPages"),
            (HfError::ForeignUrl, "ForeignUrl"),
            (HfError::Secret("x".to_owned()), "Secret"),
        ];
        for (error, code) in cases {
            let payload = payload(error);
            assert_eq!(payload.code, code);
            assert!(!payload.message.is_empty());
        }
    }

    #[test]
    fn error_payload_hf_never_contains_token() {
        let token = format!("hf_{}", "a1B2".repeat(9));
        let error = HfError::Secret(format!("fallo del llavero con {token}"));
        let json = serde_json::to_string(&payload(error)).expect("serializa");
        assert!(!json.contains(&token));
    }

    fn llama_summary() -> ModelSummary {
        ModelSummary {
            id: "bartowski/Meta-Llama-3.1-8B-Instruct-GGUF".to_owned(),
            downloads: 1000,
            likes: 50,
            gated: Gated::Manual,
            last_modified: Some("2024-08-01T00:00:00.000Z".to_owned()),
            pipeline_tag: None,
            gguf: Some(GgufInfo {
                total: Some(8_030_261_312),
                architecture: Some("llama".to_owned()),
                context_length: Some(131_072),
            }),
        }
    }

    // La UI (`ui/src/lib/hf/types.ts`) espeja estas formas: si cambian, hay que actualizarla.
    #[test]
    fn json_search_page_para_la_ui() {
        let page = SearchPageDto {
            items: vec![SearchItem {
                model: llama_summary(),
                hint: Some(FitHint {
                    estimate: MemoryEstimate {
                        bytes: 6_481_006_131,
                        kv_included: true,
                    },
                    fit: Some(Fit::Gpu),
                }),
            }],
            next: Some("https://huggingface.co/api/models?cursor=abc".to_owned()),
        };
        assert_eq!(
            serde_json::to_string(&page).expect("serializa"),
            concat!(
                r#"{"items":[{"model":{"id":"bartowski/Meta-Llama-3.1-8B-Instruct-GGUF","#,
                r#""downloads":1000,"likes":50,"gated":"Manual","#,
                r#""last_modified":"2024-08-01T00:00:00.000Z","pipeline_tag":null,"#,
                r#""gguf":{"total":8030261312,"architecture":"llama","context_length":131072}},"#,
                r#""hint":{"estimate":{"bytes":6481006131,"kv_included":true},"fit":"Gpu"}}],"#,
                r#""next":"https://huggingface.co/api/models?cursor=abc"}"#
            )
        );
    }

    #[test]
    fn search_page_dto_convierte_la_url_siguiente_en_texto() {
        let next = Url::parse("https://huggingface.co/api/models?cursor=abc").expect("url");
        let dto = SearchPageDto::from(SearchResults {
            items: Vec::new(),
            next: Some(next),
        });
        assert_eq!(
            dto.next.as_deref(),
            Some("https://huggingface.co/api/models?cursor=abc")
        );
    }

    #[test]
    fn json_search_item_without_hint() {
        let mut model = llama_summary();
        model.gguf = None;
        let item = SearchItem { model, hint: None };
        let value = serde_json::to_value(&item).expect("serializa");
        assert_eq!(value["model"]["gguf"], serde_json::Value::Null);
        assert_eq!(value["hint"], serde_json::Value::Null);
    }

    fn rtx_4090_32gib() -> MemoryBudget {
        MemoryBudget {
            gpu_bytes: Some(20_605_776_691),
            ram_bytes: Some(27_487_790_694),
            unified: false,
            usable_percent: 80,
        }
    }

    #[test]
    fn json_model_detail_para_la_ui() {
        let part = |path: &str| GgufPart {
            path: path.to_owned(),
            size: 20_000_000_000,
            sha256: Some("abc".to_owned()),
        };
        let detail = ModelDetail {
            info: ModelInfo {
                id: "ana/modelo-GGUF".to_owned(),
                gated: Gated::Open,
                downloads: 10,
                likes: 2,
                gguf: Some(GgufInfo {
                    total: Some(70_000_000_000),
                    architecture: Some("llama".to_owned()),
                    context_length: Some(8192),
                }),
                license: Some("mit".to_owned()),
            },
            ctx: 8192,
            budget: rtx_4090_32gib(),
            variants: vec![VariantView {
                variant: GgufVariant {
                    name: "Q8_0".to_owned(),
                    quant: Some("Q8_0".to_owned()),
                    parts: vec![part("m-00001-of-00002.gguf"), part("m-00002-of-00002.gguf")],
                    total_bytes: 40_000_000_000,
                    complete: true,
                },
                estimate: MemoryEstimate {
                    bytes: 41_000_000_000,
                    kv_included: true,
                },
                fit: Some(Fit::NoFit),
                recommended: false,
            }],
        };
        let sha = |path: &str| json!({"path": path, "size": 20_000_000_000_u64, "sha256": "abc"});
        assert_eq!(
            serde_json::to_value(&detail).expect("serializa"),
            json!({
                "info": {
                    "id": "ana/modelo-GGUF", "gated": "Open", "downloads": 10, "likes": 2,
                    "gguf": {"total": 70_000_000_000_u64, "architecture": "llama", "context_length": 8192},
                    "license": "mit"
                },
                "ctx": 8192,
                "budget": {
                    "gpu_bytes": 20_605_776_691_u64, "ram_bytes": 27_487_790_694_u64,
                    "unified": false, "usable_percent": 80
                },
                "variants": [{
                    "variant": {
                        "name": "Q8_0", "quant": "Q8_0",
                        "parts": [sha("m-00001-of-00002.gguf"), sha("m-00002-of-00002.gguf")],
                        "total_bytes": 40_000_000_000_u64, "complete": true
                    },
                    "estimate": {"bytes": 41_000_000_000_u64, "kv_included": true},
                    "fit": "NoFit",
                    "recommended": false
                }]
            })
        );
    }

    #[test]
    fn json_fit_variants() {
        let fits = [Some(Fit::Gpu), Some(Fit::Ram), Some(Fit::NoFit), None];
        assert_eq!(
            serde_json::to_string(&fits).expect("serializa"),
            r#"["Gpu","Ram","NoFit",null]"#
        );
    }

    #[test]
    fn json_budget_unknown() {
        assert_eq!(
            serde_json::to_string(&MemoryBudget::unknown()).expect("serializa"),
            r#"{"gpu_bytes":null,"ram_bytes":null,"unified":false,"usable_percent":80}"#
        );
    }

    #[test]
    fn json_token_status_para_la_ui() {
        let status = TokenStatus {
            present: true,
            user: Some("ana".to_owned()),
            invalid: false,
        };
        assert_eq!(
            serde_json::to_string(&status).expect("serializa"),
            r#"{"present":true,"user":"ana","invalid":false}"#
        );
    }

    #[test]
    fn sort_by_deserializa() {
        let parse = |text: &str| serde_json::from_str::<SortBy>(text);
        assert_eq!(
            parse("\"downloads\"").expect("downloads"),
            SortBy::Downloads
        );
        assert_eq!(parse("\"likes\"").expect("likes"), SortBy::Likes);
        assert!(parse("\"x\"").is_err());
    }
}
