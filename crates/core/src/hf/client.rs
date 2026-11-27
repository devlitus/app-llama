//! Cliente HTTP de la API de Hugging Face: búsqueda paginada y mapeo de errores.
use reqwest::header::{LINK, RETRY_AFTER};
use reqwest::{Response, StatusCode, Url};

use super::error::HfError;
use super::model::ModelSummary;
use super::token::HfToken;
use super::url::{SearchParams, is_trusted_target, parse_next_link, search_url};

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

    async fn fetch_search_page(&self, url: &Url) -> Result<SearchPage, HfError> {
        let response = self.get(url).await?;
        let next = next_page_url(&response, &self.base_url);
        let body = response.bytes().await.map_err(|_| HfError::Network)?;
        let items = serde_json::from_slice(&body).map_err(|e| HfError::Parse(e.to_string()))?;
        Ok(SearchPage { items, next })
    }

    /// Es el único punto que envía peticiones: el token solo viaja a destinos
    /// de confianza, nunca a otro origen.
    async fn get(&self, url: &Url) -> Result<Response, HfError> {
        if !is_trusted_target(url, &self.base_url) {
            return Err(HfError::ForeignUrl);
        }
        let mut request = self.http.get(url.clone());
        if let Some(token) = &self.token {
            request = request.bearer_auth(token.expose());
        }
        let response = request.send().await.map_err(|_| HfError::Network)?;
        check_status(response)
    }
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
