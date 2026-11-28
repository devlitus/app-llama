//! Política de autenticación de las descargas: el token de Hugging Face solo
//! viaja al origen de HF, nunca a la CDN ni a otro host.

use reqwest::Url;
use reqwest::header::HeaderValue;

use crate::hf::HfToken;
use crate::hf::url::is_trusted_target;

/// Fuente del token vigente. Quien la implementa devuelve el que tiene en
/// memoria y no consulta el llavero, para no bloquear una descarga.
pub trait TokenProvider: Send + Sync {
    fn token(&self) -> Option<HfToken>;
}

/// `Authorization` para `url` si comparte origen con `hf_origin` y hay token.
/// Una URL ilegible no recibe cabecera.
pub fn auth_header_for(
    url: &str,
    hf_origin: &Url,
    provider: &dyn TokenProvider,
) -> Option<HeaderValue> {
    let url = Url::parse(url).ok()?;
    if !is_trusted_target(&url, hf_origin) {
        return None;
    }
    bearer(&provider.token()?)
}

fn bearer(token: &HfToken) -> Option<HeaderValue> {
    let mut value = HeaderValue::from_str(&format!("Bearer {}", token.expose())).ok()?;
    value.set_sensitive(true);
    Some(value)
}

/// Cierre que recalcula la cabecera en cada llamada, de modo que un cambio o
/// borrado del token se nota en el siguiente intento de descarga.
pub fn auth_source<'a>(
    url: &'a str,
    hf_origin: &'a Url,
    provider: &'a dyn TokenProvider,
) -> impl Fn() -> Option<HeaderValue> + Send + Sync + 'a {
    move || auth_header_for(url, hf_origin, provider)
}

#[cfg(test)]
mod tests {
    use reqwest::Url;
    use tokio_util::sync::CancellationToken;
    use wiremock::matchers::{header, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::download::test_support::{FakeTokens, file_spec, hf_token, pattern_bytes};
    use crate::download::{
        DownloadError, FailureKind, FakeSleeper, HttpFetcher, RetryPolicy, TransferObserver,
        TransferOutcome, transfer_with_retry,
    };

    const KIB: usize = 1024;

    fn hf_origin() -> Url {
        Url::parse("https://huggingface.co").expect("url")
    }

    fn header_for(url: &str, tokens: &FakeTokens) -> Option<HeaderValue> {
        auth_header_for(url, &hf_origin(), tokens)
    }

    fn with_token() -> FakeTokens {
        FakeTokens::new(Some(hf_token('A')))
    }

    #[test]
    fn cabecera_solo_para_origen_hf() {
        let value = header_for("https://huggingface.co/x", &with_token()).expect("cabecera");

        assert_eq!(
            value.to_str().expect("ascii"),
            format!("Bearer {}", hf_token('A').expose())
        );
        assert!(value.is_sensitive());
    }

    #[test]
    fn sin_cabecera_otro_host() {
        assert!(header_for("https://cdn-lfs.example/x", &with_token()).is_none());
    }

    #[test]
    fn sin_cabecera_host_parecido() {
        assert!(header_for("https://huggingface.co.evil.test/x", &with_token()).is_none());
    }

    #[test]
    fn sin_cabecera_otro_puerto_o_esquema() {
        let tokens = with_token();

        assert!(header_for("http://huggingface.co/x", &tokens).is_none());
        assert!(header_for("https://huggingface.co:8443/x", &tokens).is_none());
    }

    #[test]
    fn sin_cabecera_con_credenciales_en_la_url() {
        assert!(header_for("https://user:pw@huggingface.co/x", &with_token()).is_none());
    }

    #[test]
    fn sin_cabecera_url_ilegible() {
        assert!(header_for("no es una url", &with_token()).is_none());
    }

    #[test]
    fn sin_token_sin_cabecera() {
        assert!(header_for("https://huggingface.co/x", &FakeTokens::new(None)).is_none());
    }

    #[test]
    fn auth_source_evalua_en_cada_llamada() {
        let tokens = with_token();
        let origin = hf_origin();
        let auth = auth_source("https://huggingface.co/x", &origin, &tokens);
        let first = auth().expect("primera");

        tokens.set(Some(hf_token('B')));
        let second = auth().expect("segunda");

        assert_eq!(
            first.to_str().expect("ascii"),
            format!("Bearer {}", hf_token('A').expose())
        );
        assert_eq!(
            second.to_str().expect("ascii"),
            format!("Bearer {}", hf_token('B').expose())
        );
    }

    async fn download_from(
        hf: &MockServer,
        url: String,
    ) -> (Result<TransferOutcome, DownloadError>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tmp");
        let spec = file_spec(url.clone(), &dir.path().join("m.gguf"), KIB);
        let origin = Url::parse(&hf.uri()).expect("url");
        let fetcher = HttpFetcher::new(reqwest::Client::new(), Some(origin.clone()));
        let tokens = with_token();
        let auth = auth_source(&url, &origin, &tokens);
        let outcome = transfer_with_retry(
            &fetcher,
            &spec,
            &auth,
            &RetryPolicy::default(),
            &FakeSleeper::new(),
            &CancellationToken::new(),
            TransferObserver {
                on_bytes: &mut |_| {},
                on_restart: &mut || {},
            },
        )
        .await;
        (outcome, dir)
    }

    async fn serve_status(status: u16) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn peticion_lleva_bearer() {
        let server = MockServer::start().await;
        let expected = format!("Bearer {}", hf_token('A').expose());
        Mock::given(method("GET"))
            .and(header("authorization", expected.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(pattern_bytes(KIB)))
            .mount(&server)
            .await;

        let (outcome, dir) = download_from(&server, format!("{}/f", server.uri())).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        let part = crate::download::part_path(&dir.path().join("m.gguf"));
        assert_eq!(std::fs::read(part).expect("leer"), pattern_bytes(KIB));
    }

    #[tokio::test]
    async fn redireccion_no_filtra_token() {
        let other = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(pattern_bytes(KIB)))
            .mount(&other)
            .await;
        let hf = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", format!("{}/f", other.uri())),
            )
            .mount(&hf)
            .await;

        let (outcome, _dir) = download_from(&hf, format!("{}/f", hf.uri())).await;

        assert!(matches!(outcome, Ok(TransferOutcome::Completed)));
        let at_hf = hf.received_requests().await.expect("peticiones");
        let at_other = other.received_requests().await.expect("peticiones");
        assert!(at_hf[0].headers.contains_key("authorization"));
        assert_eq!(at_other.len(), 1);
        assert!(!at_other[0].headers.contains_key("authorization"));
    }

    #[tokio::test]
    async fn gated_401_sin_reintento() {
        let server = serve_status(401).await;

        let (outcome, _dir) = download_from(&server, format!("{}/f", server.uri())).await;

        assert!(matches!(
            outcome,
            Err(DownloadError::Failed(FailureKind::Unauthorized))
        ));
        assert_eq!(
            server.received_requests().await.expect("peticiones").len(),
            1
        );
    }

    #[tokio::test]
    async fn gated_403() {
        let server = serve_status(403).await;

        let (outcome, _dir) = download_from(&server, format!("{}/f", server.uri())).await;

        assert!(matches!(
            outcome,
            Err(DownloadError::Failed(FailureKind::Gated))
        ));
    }
}
