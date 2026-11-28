//! Casos de uso del explorador: ficha del modelo y búsqueda, ambas con encaje.
use std::sync::{PoisonError, RwLock};

use reqwest::Url;
use serde::{Serialize, Serializer};

use crate::download::TokenProvider;

use super::client::HfClient;
use super::error::HfError;
use super::files::{GgufVariant, group_gguf_files};
use super::fit::{Fit, MemoryBudget, fit, recommended};
use super::hint::{FitHint, fit_hint};
use super::memory::{MemoryEstimate, estimate_memory, estimation_ctx};
use super::model::{ModelInfo, ModelSummary, WhoAmI};
use super::token::{HfToken, SecretStore};
use super::url::{DEFAULT_REVISION, RepoId, SearchParams, is_valid_revision};

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
    /// Revisión en la que se leyó el árbol; las descargas la reutilizan.
    pub revision: String,
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

/// Estado del token para la sección «Hugging Face» de Ajustes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TokenStatus {
    pub present: bool,
    pub user: Option<String>,
    pub invalid: bool,
}

pub struct HfService {
    http: reqwest::Client,
    base_url: Url,
    store: Box<dyn SecretStore + Send + Sync>,
    client: RwLock<HfClient>,
}

impl HfService {
    /// Si el almacén falla al leer, arranca sin token en vez de fallar.
    pub fn new(
        http: reqwest::Client,
        base_url: Url,
        store: Box<dyn SecretStore + Send + Sync>,
    ) -> Self {
        let token = store.get().ok().flatten();
        let client = RwLock::new(HfClient::new(http.clone(), base_url.clone(), token));
        Self {
            http,
            base_url,
            store,
            client,
        }
    }

    /// Valida el token con whoami y solo entonces lo guarda.
    pub async fn save_token(&self, raw: &str) -> Result<WhoAmI, HfError> {
        let token = HfToken::parse(raw)?;
        let who = self.current_client().whoami(&token).await?;
        self.store.set(&token)?;
        self.replace_token(Some(token));
        Ok(who)
    }

    pub fn delete_token(&self) -> Result<(), HfError> {
        self.store.delete()?;
        self.replace_token(None);
        Ok(())
    }

    /// Un token rechazado (401) se marca como inválido, pero no se borra.
    pub async fn token_status(&self) -> Result<TokenStatus, HfError> {
        let Some(token) = self.store.get()? else {
            return Ok(TokenStatus {
                present: false,
                user: None,
                invalid: false,
            });
        };
        let (user, invalid) = match self.current_client().whoami(&token).await {
            Ok(who) => (Some(who.name), false),
            Err(HfError::Unauthorized) => (None, true),
            Err(_) => (None, false),
        };
        Ok(TokenStatus {
            present: true,
            user,
            invalid,
        })
    }

    /// Clona el cliente y suelta el cerrojo antes de cualquier `await`.
    fn current_client(&self) -> HfClient {
        self.client
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Token en memoria: nunca consulta el almacén (el llavero puede bloquear).
    fn current_token(&self) -> Option<HfToken> {
        self.client
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .token()
            .cloned()
    }

    fn replace_token(&self, token: Option<HfToken>) {
        let client = HfClient::new(self.http.clone(), self.base_url.clone(), token);
        *self.client.write().unwrap_or_else(PoisonError::into_inner) = client;
    }

    pub async fn model_detail(
        &self,
        repo: &str,
        budget: &MemoryBudget,
    ) -> Result<ModelDetail, HfError> {
        let repo = RepoId::parse(repo)?;
        let client = self.current_client();
        let info = client.model_info(&repo).await?;
        let revision = pinned_revision(&info);
        let tree = client.tree(&repo, &revision).await?;
        let ctx = estimation_ctx(info.gguf.as_ref().and_then(|gguf| gguf.context_length));
        let params_total = info.gguf.as_ref().and_then(|gguf| gguf.total);
        let variants = variant_views(group_gguf_files(&tree), params_total, ctx, budget);
        Ok(ModelDetail {
            info,
            ctx,
            budget: *budget,
            variants,
            revision,
        })
    }

    pub async fn search(
        &self,
        params: &SearchParams,
        budget: &MemoryBudget,
    ) -> Result<SearchResults, HfError> {
        let client = self.current_client();
        let page = client.search(params).await?;
        Ok(with_hints(page.items, page.next, budget))
    }

    pub async fn search_next(
        &self,
        next: &Url,
        budget: &MemoryBudget,
    ) -> Result<SearchResults, HfError> {
        let client = self.current_client();
        let page = client.search_next(next).await?;
        Ok(with_hints(page.items, page.next, budget))
    }
}

impl TokenProvider for HfService {
    fn token(&self) -> Option<HfToken> {
        self.current_token()
    }
}

/// Fija la ficha al commit publicado; si falta o no es válido, usa `main`.
fn pinned_revision(info: &ModelInfo) -> String {
    info.sha
        .as_deref()
        .filter(|sha| is_valid_revision(sha))
        .unwrap_or(DEFAULT_REVISION)
        .to_owned()
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
    use crate::hf::token::{FailingStore, MemoryStore};
    use crate::hf::url::SortBy;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const INFO_GGUF: &str = include_str!("../../fixtures/hf/model_info_gguf.json");
    const INFO_NO_GGUF: &str = include_str!("../../fixtures/hf/model_info_no_gguf.json");
    const TREE: &str = include_str!("../../fixtures/hf/tree_unsloth.json");
    const TREE_FLAT: &str = include_str!("../../fixtures/hf/tree_flat.json");
    const SEARCH_PAGE: &str = include_str!("../../fixtures/hf/search_page.json");
    const WHOAMI: &str = include_str!("../../fixtures/hf/whoami.json");
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

    fn raw_token() -> String {
        format!("hf_{}", "a1B2".repeat(9))
    }

    fn token() -> HfToken {
        HfToken::parse(&raw_token()).unwrap()
    }

    fn service_with(
        server: &MockServer,
        store: impl SecretStore + Send + Sync + 'static,
    ) -> HfService {
        let base = Url::parse(&server.uri()).unwrap();
        HfService::new(reqwest::Client::new(), base, Box::new(store))
    }

    fn service_for(server: &MockServer) -> HfService {
        service_with(server, MemoryStore::default())
    }

    async fn mount_status(server: &MockServer, route: &str, status: u16, body: &str) {
        Mock::given(method("GET"))
            .and(path(route))
            .respond_with(ResponseTemplate::new(status).set_body_string(body))
            .mount(server)
            .await;
    }

    async fn mount_whoami(server: &MockServer) {
        mount_status(server, "/api/whoami-v2", 200, WHOAMI).await;
    }

    async fn mount_search_ok(server: &MockServer) {
        mount_status(server, "/api/models", 200, "[]").await;
    }

    /// Cabecera `Authorization` de la última petición a `/api/models`.
    async fn search_authorization(service: &HfService, server: &MockServer) -> Option<String> {
        service
            .search(&search_params(), &MemoryBudget::unknown())
            .await
            .unwrap();
        let requests = server.received_requests().await.unwrap();
        let last = requests
            .iter()
            .rfind(|request| request.url.path() == "/api/models")
            .unwrap();
        last.headers
            .get("authorization")
            .map(|value| value.to_str().unwrap().to_owned())
    }

    fn bearer() -> Option<String> {
        Some(format!("Bearer {}", raw_token()))
    }

    fn present_no_user(invalid: bool) -> TokenStatus {
        TokenStatus {
            present: true,
            user: None,
            invalid,
        }
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

    const COMMIT: &str = "9217f5db79a29953eb74d5343926648285ec7e67";

    fn info_with_sha(sha: &str) -> String {
        let mut value: serde_json::Value = serde_json::from_str(INFO_GGUF).unwrap();
        value["sha"] = serde_json::json!(sha);
        value.to_string()
    }

    async fn requested_tree_paths(server: &MockServer) -> Vec<String> {
        let requests = server.received_requests().await.unwrap();
        requests
            .iter()
            .map(|request| request.url.path().to_owned())
            .filter(|path| path.contains("/tree/"))
            .collect()
    }

    #[tokio::test]
    async fn model_detail_usa_sha_del_commit() {
        let server = MockServer::start().await;
        mount_json(
            &server,
            &format!("/api/models/{REPO}"),
            &info_with_sha(COMMIT),
        )
        .await;
        mount_json(&server, &format!("/api/models/{REPO}/tree/{COMMIT}"), TREE).await;
        let detail = service_for(&server)
            .model_detail(REPO, &MemoryBudget::unknown())
            .await
            .unwrap();
        assert_eq!(detail.revision, COMMIT);
        assert_eq!(
            requested_tree_paths(&server).await,
            [format!("/api/models/{REPO}/tree/{COMMIT}")]
        );
    }

    #[tokio::test]
    async fn model_detail_sin_sha_usa_main() {
        let server = MockServer::start().await;
        mount_json(&server, &format!("/api/models/{REPO}"), INFO_GGUF).await;
        mount_json(&server, &format!("/api/models/{REPO}/tree/main"), TREE).await;
        let detail = service_for(&server)
            .model_detail(REPO, &MemoryBudget::unknown())
            .await
            .unwrap();
        assert_eq!(detail.revision, "main");
        assert_eq!(
            requested_tree_paths(&server).await,
            [format!("/api/models/{REPO}/tree/main")]
        );
    }

    #[tokio::test]
    async fn model_detail_sha_invalido_usa_main() {
        let detail = detail(&info_with_sha("../x"), TREE, &MemoryBudget::unknown()).await;
        assert_eq!(detail.revision, "main");
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

    #[tokio::test]
    async fn save_token_valid_stores() {
        let server = MockServer::start().await;
        mount_whoami(&server).await;
        let service = service_for(&server);
        let who = service.save_token(&raw_token()).await.unwrap();
        assert_eq!(who.name, "ejemplo");
        let stored = service.store.get().unwrap().unwrap();
        assert_eq!(stored.expose(), raw_token());
    }

    #[tokio::test]
    async fn save_token_unauthorized_not_stored() {
        let server = MockServer::start().await;
        mount_status(&server, "/api/whoami-v2", 401, "").await;
        let service = service_for(&server);
        let result = service.save_token(&raw_token()).await;
        assert!(matches!(result, Err(HfError::Unauthorized)));
        assert!(service.store.get().unwrap().is_none());
    }

    #[tokio::test]
    async fn save_token_bad_format_no_request() {
        let server = MockServer::start().await;
        let result = service_for(&server).save_token("abc").await;
        assert!(matches!(result, Err(HfError::InvalidToken)));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn save_token_store_failure() {
        let server = MockServer::start().await;
        mount_whoami(&server).await;
        let service = service_with(&server, FailingStore);
        let result = service.save_token(&raw_token()).await;
        assert!(matches!(result, Err(HfError::Secret(_))));
    }

    #[tokio::test]
    async fn save_token_then_search_uses_bearer() {
        let server = MockServer::start().await;
        mount_whoami(&server).await;
        mount_search_ok(&server).await;
        let service = service_for(&server);
        service.save_token(&raw_token()).await.unwrap();
        assert_eq!(search_authorization(&service, &server).await, bearer());
    }

    #[tokio::test]
    async fn delete_token_removes_and_drops_header() {
        let server = MockServer::start().await;
        mount_whoami(&server).await;
        mount_search_ok(&server).await;
        let service = service_for(&server);
        service.save_token(&raw_token()).await.unwrap();
        service.delete_token().unwrap();
        assert!(service.store.get().unwrap().is_none());
        assert_eq!(search_authorization(&service, &server).await, None);
    }

    #[tokio::test]
    async fn delete_token_absent_ok() {
        let server = MockServer::start().await;
        assert!(service_for(&server).delete_token().is_ok());
    }

    #[tokio::test]
    async fn status_no_token() {
        let server = MockServer::start().await;
        let status = service_for(&server).token_status().await.unwrap();
        assert_eq!(
            status,
            TokenStatus {
                present: false,
                user: None,
                invalid: false
            }
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn status_valid_token() {
        let server = MockServer::start().await;
        mount_whoami(&server).await;
        let service = service_with(&server, MemoryStore::with_token(token()));
        assert_eq!(
            service.token_status().await.unwrap(),
            TokenStatus {
                present: true,
                user: Some("ejemplo".to_owned()),
                invalid: false
            }
        );
    }

    #[tokio::test]
    async fn status_revoked_token() {
        let server = MockServer::start().await;
        mount_status(&server, "/api/whoami-v2", 401, "").await;
        let service = service_with(&server, MemoryStore::with_token(token()));
        assert_eq!(service.token_status().await.unwrap(), present_no_user(true));
        assert!(service.store.get().unwrap().is_some());
    }

    #[tokio::test]
    async fn status_network_error() {
        // Un puerto liberado de verdad: `MockServer` seguiría escuchando desde su pool.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let base = Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap();
        let store = MemoryStore::with_token(token());
        let service = HfService::new(reqwest::Client::new(), base, Box::new(store));
        assert_eq!(
            service.token_status().await.unwrap(),
            present_no_user(false)
        );
    }

    #[tokio::test]
    async fn service_loads_stored_token_on_new() {
        let server = MockServer::start().await;
        mount_search_ok(&server).await;
        let service = service_with(&server, MemoryStore::with_token(token()));
        assert_eq!(search_authorization(&service, &server).await, bearer());
    }

    #[tokio::test]
    async fn service_new_store_failure_starts_without_token() {
        let server = MockServer::start().await;
        mount_search_ok(&server).await;
        let service = service_with(&server, FailingStore);
        assert_eq!(search_authorization(&service, &server).await, None);
    }

    #[test]
    fn hf_service_token_en_memoria() {
        let service = HfService::new(
            reqwest::Client::new(),
            Url::parse("http://127.0.0.1:1/").unwrap(),
            Box::new(MemoryStore::with_token(token())),
        );
        service.store.delete().unwrap();
        assert_eq!(service.current_token().unwrap().expose(), raw_token());
    }

    #[test]
    fn hf_service_token_tras_borrar() {
        let service = HfService::new(
            reqwest::Client::new(),
            Url::parse("http://127.0.0.1:1/").unwrap(),
            Box::new(MemoryStore::with_token(token())),
        );
        service.delete_token().unwrap();
        assert!(service.current_token().is_none());
    }
}
