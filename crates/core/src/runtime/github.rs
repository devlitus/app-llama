//! Cliente de la API de releases de GitHub para llama.cpp.
use async_trait::async_trait;
use reqwest::StatusCode;
use reqwest::header::HeaderMap;

use crate::runtime::{Release, RuntimeError, parse_releases};

const RELEASES_PATH: &str = "/repos/ggml-org/llama.cpp/releases";
const RELEASES_PER_PAGE: u32 = 10;
const USER_AGENT: &str = "app-llama";
const ACCEPT: &str = "application/vnd.github+json";
const API_VERSION: &str = "2022-11-28";
const RATE_LIMIT_REMAINING: &str = "x-ratelimit-remaining";
const RATE_LIMIT_RESET: &str = "x-ratelimit-reset";

#[async_trait]
pub trait ReleaseSource: Send + Sync {
    async fn list_releases(&self) -> Result<Vec<Release>, RuntimeError>;
}

pub struct GitHubReleaseSource {
    client: reqwest::Client,
    base_url: String,
}

impl GitHubReleaseSource {
    pub fn new(client: reqwest::Client, base_url: impl Into<String>) -> Self {
        Self {
            client,
            base_url: base_url.into(),
        }
    }
}

#[async_trait]
impl ReleaseSource for GitHubReleaseSource {
    async fn list_releases(&self) -> Result<Vec<Release>, RuntimeError> {
        let response = self
            .client
            .get(format!(
                "{}{RELEASES_PATH}?per_page={RELEASES_PER_PAGE}",
                self.base_url
            ))
            .header("User-Agent", USER_AGENT)
            .header("Accept", ACCEPT)
            .header("X-GitHub-Api-Version", API_VERSION)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(error_for_status(status, response.headers()));
        }
        parse_releases(&response.text().await?)
    }
}

fn error_for_status(status: StatusCode, headers: &HeaderMap) -> RuntimeError {
    let limited = matches!(
        status,
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
    ) && header_str(headers, RATE_LIMIT_REMAINING) == Some("0");
    if limited {
        let reset_epoch = header_str(headers, RATE_LIMIT_RESET).and_then(|v| v.parse().ok());
        return RuntimeError::RateLimited { reset_epoch };
    }
    RuntimeError::HttpStatus(status.as_u16())
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/releases_b12780.json"
    ));

    async fn source_with(server: &MockServer, response: ResponseTemplate) -> GitHubReleaseSource {
        Mock::given(method("GET"))
            .and(path(RELEASES_PATH))
            .respond_with(response)
            .mount(server)
            .await;
        GitHubReleaseSource::new(reqwest::Client::new(), server.uri())
    }

    #[tokio::test]
    async fn lista_releases_ok() {
        let server = MockServer::start().await;
        let source =
            source_with(&server, ResponseTemplate::new(200).set_body_string(FIXTURE)).await;
        assert_eq!(source.list_releases().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn lista_releases_envia_cabeceras() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(RELEASES_PATH))
            .and(query_param("per_page", "10"))
            .and(header("User-Agent", "app-llama"))
            .and(header("Accept", "application/vnd.github+json"))
            .and(header("X-GitHub-Api-Version", "2022-11-28"))
            .respond_with(ResponseTemplate::new(200).set_body_string(FIXTURE))
            .mount(&server)
            .await;
        let source = GitHubReleaseSource::new(reqwest::Client::new(), server.uri());
        assert!(source.list_releases().await.is_ok());
    }

    #[tokio::test]
    async fn lista_releases_rate_limit() {
        let server = MockServer::start().await;
        let response = ResponseTemplate::new(403)
            .insert_header("x-ratelimit-remaining", "0")
            .insert_header("x-ratelimit-reset", "1764150000");
        let source = source_with(&server, response).await;
        assert!(matches!(
            source.list_releases().await,
            Err(RuntimeError::RateLimited {
                reset_epoch: Some(1764150000)
            })
        ));
    }

    #[tokio::test]
    async fn lista_releases_429_rate_limit() {
        let server = MockServer::start().await;
        let response = ResponseTemplate::new(429).insert_header("x-ratelimit-remaining", "0");
        let source = source_with(&server, response).await;
        assert!(matches!(
            source.list_releases().await,
            Err(RuntimeError::RateLimited { reset_epoch: None })
        ));
    }

    #[tokio::test]
    async fn lista_releases_403_sin_rate_limit() {
        let server = MockServer::start().await;
        let source = source_with(&server, ResponseTemplate::new(403)).await;
        assert!(matches!(
            source.list_releases().await,
            Err(RuntimeError::HttpStatus(403))
        ));
    }

    #[tokio::test]
    async fn lista_releases_500() {
        let server = MockServer::start().await;
        let source = source_with(&server, ResponseTemplate::new(500)).await;
        assert!(matches!(
            source.list_releases().await,
            Err(RuntimeError::HttpStatus(500))
        ));
    }

    #[tokio::test]
    async fn lista_releases_json_roto() {
        let server = MockServer::start().await;
        let source = source_with(
            &server,
            ResponseTemplate::new(200).set_body_string("<html>"),
        )
        .await;
        assert!(matches!(
            source.list_releases().await,
            Err(RuntimeError::ReleaseParse(_))
        ));
    }
}
