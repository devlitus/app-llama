//! Cliente HTTP de la API de Hugging Face: búsqueda paginada y mapeo de errores.
use reqwest::header::{LINK, RETRY_AFTER};
use reqwest::{Response, StatusCode, Url};
use serde::de::DeserializeOwned;

use super::error::HfError;
use super::model::{ModelInfo, ModelSummary, TreeEntry, WhoAmI};
use super::token::HfToken;
use super::url::{
    RepoId, SearchParams, is_trusted_target, model_info_url, parse_next_link, search_url, tree_url,
    whoami_url,
};

pub const MAX_TREE_PAGES: usize = 20;

#[derive(Debug, Clone)]
pub struct HfClient {
    http: reqwest::Client,
    base_url: Url,
    token: Option<HfToken>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPage {
    pub items: Vec<ModelSummary>,
    pub next: Option<Url>,
}

impl HfClient {
    pub fn new(http: reqwest::Client, base_url: Url, token: Option<HfToken>) -> Self {
        Self {
            http,
            base_url,
            token,
        }
    }

    pub async fn search(&self, params: &SearchParams) -> Result<SearchPage, HfError> {
        self.fetch_search_page(&search_url(&self.base_url, params))
            .await
    }

    pub async fn search_next(&self, next: &Url) -> Result<SearchPage, HfError> {
        self.fetch_search_page(next).await
    }

    pub async fn model_info(&self, repo: &RepoId) -> Result<ModelInfo, HfError> {
        let (info, _) = self.get_json(&model_info_url(&self.base_url, repo)).await?;
        Ok(info)
    }

    /// Valida `token` contra la API, no el del cliente: sirve para probar un
    /// token antes de guardarlo.
    pub async fn whoami(&self, token: &HfToken) -> Result<WhoAmI, HfError> {
        let url = whoami_url(&self.base_url);
        let response = self.get_with(&url, Some(token)).await?;
        parse_body(response).await
    }

    /// Sigue `Link rel="next"` hasta `MAX_TREE_PAGES` páginas.
    pub async fn tree(&self, repo: &RepoId) -> Result<Vec<TreeEntry>, HfError> {
        let mut entries = Vec::new();
        let mut url = tree_url(&self.base_url, repo);
        for _ in 0..MAX_TREE_PAGES {
            let (page, next): (Vec<TreeEntry>, _) = self.get_json(&url).await?;
            entries.extend(page);
            match next {
                Some(next_url) => url = next_url,
                None => return Ok(entries),
            }
        }
        Err(HfError::TooManyPages)
    }

    async fn fetch_search_page(&self, url: &Url) -> Result<SearchPage, HfError> {
        let (items, next) = self.get_json(url).await?;
        Ok(SearchPage { items, next })
    }

    async fn get_json<T: DeserializeOwned>(&self, url: &Url) -> Result<(T, Option<Url>), HfError> {
        let response = self.get_with(url, self.token.as_ref()).await?;
        let next = next_page_url(&response, &self.base_url);
        let value = parse_body(response).await?;
        Ok((value, next))
    }

    /// Es el único punto que envía peticiones: el token solo viaja a destinos
    /// de confianza, nunca a otro origen.
    async fn get_with(&self, url: &Url, token: Option<&HfToken>) -> Result<Response, HfError> {
        if !is_trusted_target(url, &self.base_url) {
            return Err(HfError::ForeignUrl);
        }
        let mut request = self.http.get(url.clone());
        if let Some(token) = token {
            request = request.bearer_auth(token.expose());
        }
        let response = request.send().await.map_err(|_| HfError::Network)?;
        check_status(response)
    }
}

async fn parse_body<T: DeserializeOwned>(response: Response) -> Result<T, HfError> {
    let body = response.bytes().await.map_err(|_| HfError::Network)?;
    serde_json::from_slice(&body).map_err(|e| HfError::Parse(e.to_string()))
}

fn next_page_url(response: &Response, base: &Url) -> Option<Url> {
    response
        .headers()
        .get_all(LINK)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|header| parse_next_link(header, base))
}

fn check_status(response: Response) -> Result<Response, HfError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    Err(match status {
        StatusCode::UNAUTHORIZED => HfError::Unauthorized,
        StatusCode::FORBIDDEN => HfError::Forbidden,
        StatusCode::NOT_FOUND => HfError::NotFound,
        StatusCode::TOO_MANY_REQUESTS => HfError::RateLimited {
            retry_after_secs: retry_after_secs(&response),
        },
        other => HfError::Status(other.as_u16()),
    })
}

fn retry_after_secs(response: &Response) -> Option<u64> {
    response
        .headers()
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hf::url::SortBy;

    const INFO_GGUF: &str = include_str!("../../fixtures/hf/model_info_gguf.json");
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const SEARCH_PAGE: &str = include_str!("../../fixtures/hf/search_page.json");

    fn token() -> HfToken {
        HfToken::parse(&format!("hf_{}", "a1B2".repeat(9))).unwrap()
    }

    fn params() -> SearchParams {
        SearchParams {
            query: "qwen".to_owned(),
            sort: SortBy::Downloads,
            limit: 20,
        }
    }

    fn client_for(server: &MockServer, token: Option<HfToken>) -> HfClient {
        let base = Url::parse(&server.uri()).unwrap();
        HfClient::new(reqwest::Client::new(), base, token)
    }

    async fn mount_search(server: &MockServer, response: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path("/api/models"))
            .respond_with(response)
            .mount(server)
            .await;
    }

    async fn search_status(status: ResponseTemplate) -> Result<SearchPage, HfError> {
        let server = MockServer::start().await;
        mount_search(&server, status).await;
        client_for(&server, None).search(&params()).await
    }

    async fn received_authorization(server: &MockServer) -> Option<String> {
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        requests[0]
            .headers
            .get("authorization")
            .map(|value| value.to_str().unwrap().to_owned())
    }

    const WHOAMI: &str = include_str!("../../fixtures/hf/whoami.json");

    #[tokio::test]
    async fn whoami_uses_given_token_not_client_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/whoami-v2"))
            .respond_with(ResponseTemplate::new(200).set_body_string(WHOAMI))
            .mount(&server)
            .await;
        let other = HfToken::parse(&format!("hf_{}", "z9Y8".repeat(9))).unwrap();
        let who = client_for(&server, Some(token()))
            .whoami(&other)
            .await
            .unwrap();
        assert_eq!(who.name, "ejemplo");
        let sent = received_authorization(&server).await;
        assert_eq!(sent, Some(format!("Bearer {}", other.expose())));
    }

    #[tokio::test]
    async fn whoami_unauthorized() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let result = client_for(&server, None).whoami(&token()).await;
        assert!(matches!(result, Err(HfError::Unauthorized)));
    }

    /// `MockServer` devuelve el puerto a un pool al soltarse y sigue escuchando,
    /// así que un puerto cerrado de verdad se obtiene con un listener efímero.
    fn closed_port_base() -> Url {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        Url::parse(&format!("http://127.0.0.1:{port}/")).unwrap()
    }

    #[tokio::test]
    async fn whoami_network_error() {
        let client = HfClient::new(reqwest::Client::new(), closed_port_base(), None);
        let result = client.whoami(&token()).await;
        assert!(matches!(result, Err(HfError::Network)));
    }

    #[tokio::test]
    async fn search_sends_expected_query() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/models"))
            .and(query_param("search", "qwen"))
            .and(query_param("filter", "gguf"))
            .and(query_param("sort", "downloads"))
            .and(query_param("direction", "-1"))
            .and(query_param("limit", "20"))
            .and(query_param("expand[]", "gguf"))
            .and(query_param("expand[]", "pipeline_tag"))
            .respond_with(ResponseTemplate::new(200).set_body_string(SEARCH_PAGE))
            .mount(&server)
            .await;
        let page = client_for(&server, None).search(&params()).await.unwrap();
        let expected: Vec<ModelSummary> = serde_json::from_str(SEARCH_PAGE).unwrap();
        assert_eq!(page.items, expected);
        assert_eq!(page.next, None);
    }

    #[tokio::test]
    async fn search_follows_next_cursor() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/models"))
            .and(query_param("cursor", "abc"))
            .respond_with(ResponseTemplate::new(200).set_body_string("[]"))
            .mount(&server)
            .await;
        let link = format!("<{}/api/models?cursor=abc>; rel=\"next\"", server.uri());
        mount_search(
            &server,
            ResponseTemplate::new(200)
                .set_body_string(SEARCH_PAGE)
                .insert_header("Link", link.as_str()),
        )
        .await;
        let client = client_for(&server, None);
        let first = client.search(&params()).await.unwrap();
        let next = first.next.unwrap();
        assert_eq!(next.query(), Some("cursor=abc"));
        let second = client.search_next(&next).await.unwrap();
        assert!(second.items.is_empty());
        assert_eq!(second.next, None);
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn search_empty_page() {
        let page = search_status(ResponseTemplate::new(200).set_body_string("[]"))
            .await
            .unwrap();
        assert!(page.items.is_empty());
        assert_eq!(page.next, None);
    }

    #[tokio::test]
    async fn search_next_foreign_url_rejected() {
        let server = MockServer::start().await;
        let client = client_for(&server, Some(token()));
        let foreign = Url::parse("https://evil.example/api/models?cursor=abc").unwrap();
        assert!(matches!(
            client.search_next(&foreign).await,
            Err(HfError::ForeignUrl)
        ));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn search_next_url_with_credentials_rejected() {
        let server = MockServer::start().await;
        let client = client_for(&server, None);
        let mut with_credentials = Url::parse(&server.uri()).unwrap();
        with_credentials.set_username("user").unwrap();
        assert!(matches!(
            client.search_next(&with_credentials).await,
            Err(HfError::ForeignUrl)
        ));
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn client_sends_bearer_when_token() {
        let server = MockServer::start().await;
        mount_search(&server, ResponseTemplate::new(200).set_body_string("[]")).await;
        let token = token();
        client_for(&server, Some(token.clone()))
            .search(&params())
            .await
            .unwrap();
        assert_eq!(
            received_authorization(&server).await,
            Some(format!("Bearer {}", token.expose()))
        );
    }

    #[tokio::test]
    async fn client_no_auth_header_without_token() {
        let server = MockServer::start().await;
        mount_search(&server, ResponseTemplate::new(200).set_body_string("[]")).await;
        client_for(&server, None).search(&params()).await.unwrap();
        assert_eq!(received_authorization(&server).await, None);
    }

    #[tokio::test]
    async fn client_maps_status_401() {
        let result = search_status(ResponseTemplate::new(401)).await;
        assert!(matches!(result, Err(HfError::Unauthorized)));
    }

    #[tokio::test]
    async fn client_maps_status_403() {
        let result = search_status(ResponseTemplate::new(403)).await;
        assert!(matches!(result, Err(HfError::Forbidden)));
    }

    #[tokio::test]
    async fn client_maps_status_404() {
        let result = search_status(ResponseTemplate::new(404)).await;
        assert!(matches!(result, Err(HfError::NotFound)));
    }

    #[tokio::test]
    async fn client_maps_status_429_with_retry_after() {
        let response = ResponseTemplate::new(429).insert_header("Retry-After", "30");
        assert!(matches!(
            search_status(response).await,
            Err(HfError::RateLimited {
                retry_after_secs: Some(30)
            })
        ));
    }

    #[tokio::test]
    async fn client_maps_status_429_without_retry_after() {
        assert!(matches!(
            search_status(ResponseTemplate::new(429)).await,
            Err(HfError::RateLimited {
                retry_after_secs: None
            })
        ));
    }

    #[tokio::test]
    async fn client_maps_status_429_with_non_numeric_retry_after() {
        let response = ResponseTemplate::new(429)
            .insert_header("Retry-After", "Wed, 21 Oct 2026 07:28:00 GMT");
        assert!(matches!(
            search_status(response).await,
            Err(HfError::RateLimited {
                retry_after_secs: None
            })
        ));
    }

    #[tokio::test]
    async fn client_maps_status_500() {
        let result = search_status(ResponseTemplate::new(500)).await;
        assert!(matches!(result, Err(HfError::Status(500))));
    }

    #[tokio::test]
    async fn client_maps_invalid_json() {
        let response = ResponseTemplate::new(200).set_body_string(r#"{"no":"array"}"#);
        assert!(matches!(
            search_status(response).await,
            Err(HfError::Parse(_))
        ));
    }

    fn repo() -> RepoId {
        RepoId::parse("a/b").unwrap()
    }

    fn tree_page(name: &str) -> String {
        format!(r#"[{{"type":"file","path":"{name}.gguf","size":10}}]"#)
    }

    fn next_link(server: &MockServer, cursor: &str) -> String {
        format!(
            "<{}/api/models/a/b/tree/main?recursive=true&cursor={cursor}>; rel=\"next\"",
            server.uri()
        )
    }

    async fn mount_tree(server: &MockServer, response: ResponseTemplate) {
        Mock::given(method("GET"))
            .and(path("/api/models/a/b/tree/main"))
            .and(query_param("recursive", "true"))
            .respond_with(response)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn tree_requests_recursive() {
        let server = MockServer::start().await;
        mount_tree(&server, ResponseTemplate::new(200).set_body_string("[]")).await;
        let entries = client_for(&server, None).tree(&repo()).await.unwrap();
        assert!(entries.is_empty());
    }

    #[tokio::test]
    async fn tree_follows_pagination() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/models/a/b/tree/main"))
            .and(query_param("cursor", "p2"))
            .respond_with(ResponseTemplate::new(200).set_body_string(tree_page("second")))
            .mount(&server)
            .await;
        let link = next_link(&server, "p2");
        Mock::given(method("GET"))
            .and(path("/api/models/a/b/tree/main"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(tree_page("first"))
                    .insert_header("Link", link.as_str()),
            )
            .with_priority(10)
            .mount(&server)
            .await;
        let entries = client_for(&server, None).tree(&repo()).await.unwrap();
        let paths: Vec<_> = entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["first.gguf", "second.gguf"]);
    }

    #[tokio::test]
    async fn tree_too_many_pages() {
        let server = MockServer::start().await;
        let link = next_link(&server, "more");
        mount_tree(
            &server,
            ResponseTemplate::new(200)
                .set_body_string("[]")
                .insert_header("Link", link.as_str()),
        )
        .await;
        let result = client_for(&server, None).tree(&repo()).await;
        assert!(matches!(result, Err(HfError::TooManyPages)));
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), MAX_TREE_PAGES);
    }

    #[tokio::test]
    async fn model_info_requests_expands_and_parses() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/models/a/b"))
            .and(query_param("expand[]", "gguf"))
            .and(query_param("expand[]", "gated"))
            .respond_with(ResponseTemplate::new(200).set_body_string(INFO_GGUF))
            .mount(&server)
            .await;
        let info = client_for(&server, None).model_info(&repo()).await.unwrap();
        assert_eq!(info.id, "unsloth/Qwen3.8-27B-GGUF");
    }

    #[tokio::test]
    async fn client_maps_connection_refused() {
        let closed_port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let base = Url::parse(&format!("http://127.0.0.1:{closed_port}")).unwrap();
        let client = HfClient::new(reqwest::Client::new(), base, None);
        assert!(matches!(
            client.search(&params()).await,
            Err(HfError::Network)
        ));
    }
}
