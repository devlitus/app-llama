//! Casos de uso del explorador: ficha del modelo y búsqueda, ambas con encaje.
use reqwest::Url;
use serde::{Serialize, Serializer};

use super::client::HfClient;
use super::error::HfError;
use super::files::{GgufVariant, group_gguf_files};
use super::fit::{Fit, MemoryBudget, fit, recommended};
use super::hint::{FitHint, fit_hint};
use super::memory::{MemoryEstimate, estimate_memory, estimation_ctx};
use super::model::{ModelInfo, ModelSummary};
use super::url::{RepoId, SearchParams};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VariantView {
    pub variant: GgufVariant,
    pub estimate: MemoryEstimate,
    pub fit: Option<Fit>,
    pub recommended: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelDetail {
    pub info: ModelInfo,
    pub ctx: u32,
    pub budget: MemoryBudget,
    pub variants: Vec<VariantView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchItem {
    pub model: ModelSummary,
    pub hint: Option<FitHint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchResults {
    pub items: Vec<SearchItem>,
    #[serde(serialize_with = "serialize_url")]
    pub next: Option<Url>,
}

/// `reqwest::Url` no es `Serialize` sin activar el feature `serde` de `url`.
fn serialize_url<S: Serializer>(url: &Option<Url>, serializer: S) -> Result<S::Ok, S::Error> {
    match url {
        Some(url) => serializer.collect_str(url),
        None => serializer.serialize_none(),
    }
}

#[derive(Debug, Clone)]
pub struct HfService {
    client: HfClient,
}

impl HfService {
    pub fn new(client: HfClient) -> Self {
        Self { client }
    }

    pub async fn model_detail(
        &self,
        repo: &str,
        budget: &MemoryBudget,
    ) -> Result<ModelDetail, HfError> {
        let repo = RepoId::parse(repo)?;
        let info = self.client.model_info(&repo).await?;
        let tree = self.client.tree(&repo).await?;
        let ctx = estimation_ctx(info.gguf.as_ref().and_then(|gguf| gguf.context_length));
        let params_total = info.gguf.as_ref().and_then(|gguf| gguf.total);
        let variants = variant_views(group_gguf_files(&tree), params_total, ctx, budget);
        Ok(ModelDetail {
            info,
            ctx,
            budget: *budget,
            variants,
        })
    }

    pub async fn search(
        &self,
        params: &SearchParams,
        budget: &MemoryBudget,
    ) -> Result<SearchResults, HfError> {
        let page = self.client.search(params).await?;
        Ok(with_hints(page.items, page.next, budget))
    }

    pub async fn search_next(
        &self,
        next: &Url,
        budget: &MemoryBudget,
    ) -> Result<SearchResults, HfError> {
        let page = self.client.search_next(next).await?;
        Ok(with_hints(page.items, page.next, budget))
    }
}

fn with_hints(items: Vec<ModelSummary>, next: Option<Url>, budget: &MemoryBudget) -> SearchResults {
    let items = items
        .into_iter()
        .map(|model| SearchItem {
            hint: fit_hint(model.gguf.as_ref(), budget),
            model,
        })
        .collect();
    SearchResults { items, next }
}

fn variant_views(
    variants: Vec<GgufVariant>,
    params_total: Option<u64>,
    ctx: u32,
    budget: &MemoryBudget,
) -> Vec<VariantView> {
    let estimates: Vec<MemoryEstimate> = variants
        .iter()
        .map(|variant| estimate_memory(variant.total_bytes, params_total, ctx))
        .collect();
    let fits: Vec<Option<Fit>> = variants
        .iter()
        .zip(&estimates)
        .map(|(variant, estimate)| {
            variant
                .complete
                .then(|| fit(estimate.bytes, budget))
                .flatten()
        })
        .collect();
    let candidates: Vec<(&GgufVariant, Option<Fit>)> =
        variants.iter().zip(fits.iter().copied()).collect();
    let best = recommended(&candidates);
    variants
        .iter()
        .cloned()
        .zip(estimates)
        .zip(fits)
        .enumerate()
        .map(|(index, ((variant, estimate), fit))| VariantView {
            variant,
            estimate,
            fit,
            recommended: best == Some(index),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hf::url::SortBy;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const INFO_GGUF: &str = include_str!("../../fixtures/hf/model_info_gguf.json");
    const INFO_NO_GGUF: &str = include_str!("../../fixtures/hf/model_info_no_gguf.json");
    const TREE: &str = include_str!("../../fixtures/hf/tree_unsloth.json");
    const TREE_FLAT: &str = include_str!("../../fixtures/hf/tree_flat.json");
    const SEARCH_PAGE: &str = include_str!("../../fixtures/hf/search_page.json");
    const REPO: &str = "unsloth/Qwen3.8-27B-GGUF";

    fn budget(gpu: u64) -> MemoryBudget {
        MemoryBudget {
            gpu_bytes: Some(gpu),
            ram_bytes: Some(27_487_790_694),
            unified: false,
            usable_percent: 80,
        }
    }

    fn rtx_4090() -> MemoryBudget {
        budget(20_605_776_691)
    }

    fn rtx_4070() -> MemoryBudget {
        budget(10_302_888_345)
    }

    fn service_for(server: &MockServer) -> HfService {
        let base = Url::parse(&server.uri()).unwrap();
        HfService::new(HfClient::new(reqwest::Client::new(), base, None))
    }

    async fn mount_json(server: &MockServer, route: &str, body: &str) {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(server)
            .await;
    }

    async fn detail(info: &str, tree: &str, budget: &MemoryBudget) -> ModelDetail {
        let server = MockServer::start().await;
        mount_json(&server, &format!("/api/models/{REPO}"), info).await;
        mount_json(&server, &format!("/api/models/{REPO}/tree/main"), tree).await;
        service_for(&server)
            .model_detail(REPO, budget)
            .await
            .unwrap()
    }

    fn names(detail: &ModelDetail) -> Vec<&str> {
        detail
            .variants
            .iter()
            .map(|view| view.variant.name.as_str())
            .collect()
    }

    fn fits(detail: &ModelDetail) -> Vec<Option<Fit>> {
        detail.variants.iter().map(|view| view.fit).collect()
    }

    fn recommended_names(detail: &ModelDetail) -> Vec<&str> {
        detail
            .variants
            .iter()
            .filter(|view| view.recommended)
            .map(|view| view.variant.name.as_str())
            .collect()
    }

    #[tokio::test]
    async fn model_detail_assembles_variants() {
        let detail = detail(INFO_GGUF, TREE, &MemoryBudget::unknown()).await;
        assert_eq!(
            names(&detail),
            [
                "Qwen3.8-27B-UD-IQ2_XXS",
                "Qwen3.8-27B-Q4_0",
                "Qwen3.8-27B-UD-Q4_K_XL",
                "BF16/Qwen3.8-27B-BF16"
            ]
        );
        let quants: Vec<_> = detail
            .variants
            .iter()
            .map(|view| view.variant.quant.as_deref())
            .collect();
        assert_eq!(
            quants,
            [Some("IQ2_XXS"), Some("Q4_0"), Some("Q4_K_XL"), Some("BF16")]
        );
        assert_eq!(detail.variants[3].variant.parts.len(), 2);
        assert!(detail.variants.iter().all(|view| view.estimate.kv_included));
        assert_eq!(detail.ctx, 8192);
        assert_eq!(fits(&detail), [None, None, None, None]);
        assert!(recommended_names(&detail).is_empty());
    }

    #[tokio::test]
    async fn model_detail_fit_and_recommended() {
        let detail = detail(INFO_GGUF, TREE, &rtx_4090()).await;
        let bytes: Vec<_> = detail
            .variants
            .iter()
            .map(|view| view.estimate.bytes)
            .collect();
        assert_eq!(
            bytes,
            [
                11_409_833_984,
                20_260_269_024,
                21_863_197_600,
                58_861_525_952
            ]
        );
        assert_eq!(
            fits(&detail),
            [
                Some(Fit::Gpu),
                Some(Fit::Gpu),
                Some(Fit::Ram),
                Some(Fit::NoFit)
            ]
        );
        assert_eq!(recommended_names(&detail), ["Qwen3.8-27B-Q4_0"]);
        assert_eq!(detail.budget, rtx_4090());
    }

    #[tokio::test]
    async fn model_detail_rtx4070_recommends_largest_ram() {
        let detail = detail(INFO_GGUF, TREE, &rtx_4070()).await;
        assert_eq!(
            fits(&detail),
            [
                Some(Fit::Ram),
                Some(Fit::Ram),
                Some(Fit::Ram),
                Some(Fit::NoFit)
            ]
        );
        assert_eq!(recommended_names(&detail), ["Qwen3.8-27B-UD-Q4_K_XL"]);
    }

    #[tokio::test]
    async fn model_detail_incomplete_variant_no_fit() {
        let mut entries: Vec<serde_json::Value> = serde_json::from_str(TREE).unwrap();
        entries.retain(|entry| {
            let path = entry["path"].as_str().unwrap_or_default();
            !path.contains("MTP") && !path.ends_with("00002-of-00002.gguf")
        });
        let tree = serde_json::to_string(&entries).unwrap();
        let detail = detail(INFO_GGUF, &tree, &rtx_4090()).await;
        let bf16 = detail
            .variants
            .iter()
            .find(|view| view.variant.quant.as_deref() == Some("BF16"))
            .unwrap();
        assert!(!bf16.variant.complete);
        assert_eq!(bf16.fit, None);
        assert!(!bf16.recommended);
        assert_eq!(recommended_names(&detail), ["Qwen3.8-27B-Q4_0"]);
    }

    #[tokio::test]
    async fn model_detail_without_gguf_metadata() {
        let detail = detail(INFO_NO_GGUF, TREE_FLAT, &MemoryBudget::unknown()).await;
        assert!(!detail.variants.is_empty());
        assert!(
            detail
                .variants
                .iter()
                .all(|view| !view.estimate.kv_included)
        );
        assert_eq!(detail.ctx, 4096);
        assert!(detail.variants.iter().all(|view| view.fit.is_none()));
    }

    #[tokio::test]
    async fn model_detail_invalid_repo_no_request() {
        let server = MockServer::start().await;
        let result = service_for(&server)
            .model_detail("bad id", &MemoryBudget::unknown())
            .await;
        assert!(matches!(result, Err(HfError::InvalidRepoId)));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn model_detail_not_found() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let result = service_for(&server)
            .model_detail(REPO, &MemoryBudget::unknown())
            .await;
        assert!(matches!(result, Err(HfError::NotFound)));
    }

    fn search_params() -> SearchParams {
        SearchParams {
            query: "llama".to_owned(),
            sort: SortBy::Downloads,
            limit: 20,
        }
    }

    #[tokio::test]
    async fn search_hints_without_extra_requests() {
        let server = MockServer::start().await;
        mount_json(&server, "/api/models", SEARCH_PAGE).await;
        let results = service_for(&server)
            .search(&search_params(), &rtx_4070())
            .await
            .unwrap();
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        assert_eq!(results.items.len(), 3);
        let hint_8b = results.items[0].hint.unwrap();
        assert_eq!(
            hint_8b.estimate,
            MemoryEstimate {
                bytes: 6_481_006_131,
                kv_included: true
            }
        );
        assert_eq!(hint_8b.fit, Some(Fit::Gpu));
        let hint_1b = results.items[1].hint.unwrap();
        assert_eq!(hint_1b.estimate.bytes, 1_451_637_881);
        assert_eq!(hint_1b.fit, Some(Fit::Gpu));
        let hint_70b = results.items[2].hint.unwrap();
        assert_eq!(hint_70b.estimate.bytes, 52_761_967_679);
        assert_eq!(hint_70b.fit, Some(Fit::NoFit));
    }

    #[tokio::test]
    async fn search_item_without_gguf_has_no_hint() {
        let mut items: Vec<serde_json::Value> = serde_json::from_str(SEARCH_PAGE).unwrap();
        items.truncate(1);
        items[0].as_object_mut().unwrap().remove("gguf");
        let server = MockServer::start().await;
        mount_json(
            &server,
            "/api/models",
            &serde_json::to_string(&items).unwrap(),
        )
        .await;
        let results = service_for(&server)
            .search(&search_params(), &rtx_4070())
            .await
            .unwrap();
        assert_eq!(results.items.len(), 1);
        assert_eq!(results.items[0].hint, None);
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn search_next_computes_hints() {
        let mut items: Vec<serde_json::Value> = serde_json::from_str(SEARCH_PAGE).unwrap();
        items.truncate(1);
        let server = MockServer::start().await;
        mount_json(
            &server,
            "/api/models",
            &serde_json::to_string(&items).unwrap(),
        )
        .await;
        let next = Url::parse(&format!("{}/api/models?cursor=abc", server.uri())).unwrap();
        let results = service_for(&server)
            .search_next(&next, &rtx_4070())
            .await
            .unwrap();
        assert_eq!(results.items[0].hint.unwrap().fit, Some(Fit::Gpu));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}
