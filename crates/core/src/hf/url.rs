//! Identificador de repositorio y construcción de URLs de la API de Hugging Face.
use reqwest::Url;
use serde::Deserialize;

use super::error::HfError;

const MAX_SEGMENT_LEN: usize = 96;
const SEARCH_EXPANDS: [&str; 6] = [
    "downloads",
    "likes",
    "gated",
    "lastModified",
    "pipeline_tag",
    "gguf",
];
const MODEL_INFO_EXPANDS: [&str; 6] = [
    "gguf",
    "gated",
    "downloads",
    "likes",
    "lastModified",
    "cardData",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoId(String);

impl RepoId {
    /// Equivale a `^[A-Za-z0-9][A-Za-z0-9._-]{0,95}/[A-Za-z0-9][A-Za-z0-9._-]{0,95}$`
    /// sin `..`, para que el id nunca altere la ruta ni la query de la URL.
    pub fn parse(raw: &str) -> Result<RepoId, HfError> {
        let valid = raw
            .split_once('/')
            .is_some_and(|(owner, name)| is_valid_segment(owner) && is_valid_segment(name));
        if valid && !raw.contains("..") {
            Ok(RepoId(raw.to_owned()))
        } else {
            Err(HfError::InvalidRepoId)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_valid_segment(segment: &str) -> bool {
    let mut chars = segment.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && segment.len() <= MAX_SEGMENT_LEN
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SortBy {
    Downloads,
    Likes,
}

impl SortBy {
    fn as_param(self) -> &'static str {
        match self {
            SortBy::Downloads => "downloads",
            SortBy::Likes => "likes",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchParams {
    pub query: String,
    pub sort: SortBy,
    pub limit: u32,
}

pub fn search_url(base: &Url, params: &SearchParams) -> Url {
    let mut pairs = Vec::new();
    let query = params.query.trim();
    if !query.is_empty() {
        pairs.push(format!("search={}", percent_encode(query)));
    }
    pairs.push("filter=gguf".to_owned());
    pairs.push(format!("sort={}", params.sort.as_param()));
    pairs.push("direction=-1".to_owned());
    pairs.push(format!("limit={}", params.limit));
    pairs.extend(expand_pairs(&SEARCH_EXPANDS));
    endpoint(base, &["api", "models"], &pairs.join("&"))
}

pub fn model_info_url(base: &Url, repo: &RepoId) -> Url {
    let query = expand_pairs(&MODEL_INFO_EXPANDS).join("&");
    endpoint(base, &["api", "models", repo.as_str()], &query)
}

pub fn tree_url(base: &Url, repo: &RepoId) -> Url {
    endpoint(
        base,
        &["api", "models", repo.as_str(), "tree", "main"],
        "recursive=true",
    )
}

/// Extrae la URL con `rel="next"` y la descarta si no comparte esquema, host y
/// puerto con `base`, para no enviar nunca el token a otro destino.
pub fn parse_next_link(header: &str, base: &Url) -> Option<Url> {
    header
        .split(',')
        .filter_map(next_target)
        .filter_map(|target| Url::parse(target).ok())
        .find(|url| same_origin(url, base))
}

fn next_target(link: &str) -> Option<&str> {
    let (target, params) = link.trim().strip_prefix('<')?.split_once('>')?;
    params
        .split(';')
        .map(|param| param.trim().replace('"', ""))
        .any(|param| param.eq_ignore_ascii_case("rel=next"))
        .then_some(target)
}

fn same_origin(a: &Url, b: &Url) -> bool {
    a.scheme() == b.scheme()
        && a.host_str() == b.host_str()
        && a.port_or_known_default() == b.port_or_known_default()
}

fn expand_pairs(fields: &[&str]) -> Vec<String> {
    fields
        .iter()
        .map(|field| format!("expand[]={field}"))
        .collect()
}

/// Los segmentos se añaden uno a uno, así el `/` del repo id no se codifica
/// como parte de un único segmento. `base` es siempre `http(s)`, que admite
/// segmentos de ruta.
fn endpoint(base: &Url, segments: &[&str], query: &str) -> Url {
    let mut url = base.clone();
    if let Ok(mut path) = url.path_segments_mut() {
        path.pop_if_empty();
        for segment in segments.iter().flat_map(|s| s.split('/')) {
            path.push(segment);
        }
    }
    url.set_query(Some(query));
    url
}

fn percent_encode(text: &str) -> String {
    text.bytes().fold(String::new(), |mut out, byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HF: &str = "https://huggingface.co";

    fn base() -> Url {
        Url::parse(HF).unwrap()
    }

    fn repo(raw: &str) -> RepoId {
        RepoId::parse(raw).unwrap()
    }

    fn params(query: &str, sort: SortBy, limit: u32) -> SearchParams {
        SearchParams {
            query: query.to_owned(),
            sort,
            limit,
        }
    }

    fn assert_invalid(raw: &str) {
        assert!(matches!(RepoId::parse(raw), Err(HfError::InvalidRepoId)));
    }

    #[test]
    fn repo_id_valid() {
        assert!(RepoId::parse("unsloth/Qwen3.8-27B-GGUF").is_ok());
    }

    #[test]
    fn repo_id_missing_owner() {
        assert_invalid("Qwen3-8B");
    }

    #[test]
    fn repo_id_three_segments() {
        assert_invalid("a/b/c");
    }

    #[test]
    fn repo_id_dotdot() {
        assert_invalid("a/..");
        assert_invalid("a/b..c");
    }

    #[test]
    fn repo_id_query_injection() {
        assert_invalid("a/b?x=1");
    }

    #[test]
    fn repo_id_empty() {
        assert_invalid("");
    }

    #[test]
    fn repo_id_rejects_edges() {
        assert_invalid("/b");
        assert_invalid("a/");
        assert_invalid("-a/b");
        assert_invalid("a/.b");
        assert_invalid("a/b c");
        assert_invalid("a/bé");
        assert_invalid(&format!("{}/b", "a".repeat(97)));
        assert_invalid(&format!("a/{}", "b".repeat(97)));
    }

    #[test]
    fn repo_id_accepts_length_limit() {
        assert!(RepoId::parse(&format!("{}/{}", "a".repeat(96), "b".repeat(96))).is_ok());
    }

    #[test]
    fn repo_id_as_str_roundtrip() {
        let id = RepoId::parse("unsloth/Qwen3.8-27B-GGUF").unwrap();
        assert_eq!(id.as_str(), "unsloth/Qwen3.8-27B-GGUF");
    }

    #[test]
    fn search_url_sort_downloads() {
        let url = search_url(&base(), &params("qwen", SortBy::Downloads, 20));
        let text = url.as_str();
        assert!(text.starts_with("https://huggingface.co/api/models?"));
        for expected in [
            "search=qwen",
            "filter=gguf",
            "sort=downloads",
            "direction=-1",
            "limit=20",
            "expand[]=downloads",
            "expand[]=likes",
            "expand[]=gated",
            "expand[]=lastModified",
            "expand[]=pipeline_tag",
            "expand[]=gguf",
        ] {
            assert!(text.contains(expected), "falta {expected} en {text}");
        }
        assert_eq!(text.matches("expand[]=").count(), 6);
    }

    #[test]
    fn search_url_sort_likes() {
        let url = search_url(&base(), &params("qwen", SortBy::Likes, 20));
        assert!(url.as_str().contains("sort=likes"));
    }

    #[test]
    fn search_url_encodes_query() {
        let url = search_url(&base(), &params("llama 3&x", SortBy::Downloads, 20));
        let text = url.as_str();
        assert!(text.contains("search=llama+3%26x") || text.contains("search=llama%203%26x"));
        assert!(!text.contains("&x"));
    }

    #[test]
    fn search_url_blank_query_omits_search() {
        let url = search_url(&base(), &params("  ", SortBy::Downloads, 20));
        assert!(!url.as_str().contains("search="));
    }

    #[test]
    fn search_url_keeps_base_path_and_port() {
        let base = Url::parse("http://127.0.0.1:8080/").unwrap();
        let url = search_url(&base, &params("q", SortBy::Downloads, 5));
        assert!(
            url.as_str()
                .starts_with("http://127.0.0.1:8080/api/models?")
        );
    }

    #[test]
    fn tree_url_recursive() {
        let url = tree_url(&base(), &repo("a/b"));
        assert!(
            url.as_str()
                .ends_with("/api/models/a/b/tree/main?recursive=true")
        );
    }

    #[test]
    fn model_info_url_expands() {
        let url = model_info_url(&base(), &repo("a/b"));
        let text = url.as_str();
        assert!(text.contains("/api/models/a/b?"));
        assert!(text.contains("expand[]=gguf"));
        assert!(text.contains("expand[]=gated"));
    }

    #[test]
    fn sort_by_deserializes_lowercase() {
        let sort: SortBy = serde_json::from_str("\"likes\"").unwrap();
        assert_eq!(sort, SortBy::Likes);
    }

    #[test]
    fn parse_next_link_simple() {
        let header = r#"<https://huggingface.co/api/models?cursor=abc>; rel="next""#;
        let next = parse_next_link(header, &base()).unwrap();
        assert_eq!(next.query(), Some("cursor=abc"));
    }

    #[test]
    fn parse_next_link_multiple_rels() {
        let header = r#"<https://huggingface.co/api/models?cursor=p>; rel="prev", <https://huggingface.co/api/models?cursor=n>; rel="next""#;
        let next = parse_next_link(header, &base()).unwrap();
        assert_eq!(next.query(), Some("cursor=n"));
    }

    #[test]
    fn parse_next_link_absent() {
        let header = r#"<https://huggingface.co/api/models?cursor=p>; rel="prev""#;
        assert_eq!(parse_next_link(header, &base()), None);
    }

    #[test]
    fn parse_next_link_other_host_ignored() {
        let header = r#"<https://evil.example/api>; rel="next""#;
        assert_eq!(parse_next_link(header, &base()), None);
    }

    #[test]
    fn parse_next_link_other_scheme_ignored() {
        let header = r#"<http://huggingface.co/api/models?cursor=abc>; rel="next""#;
        assert_eq!(parse_next_link(header, &base()), None);
    }

    #[test]
    fn parse_next_link_other_port_ignored() {
        let header = r#"<https://huggingface.co:8443/api?cursor=abc>; rel="next""#;
        assert_eq!(parse_next_link(header, &base()), None);
    }

    #[test]
    fn parse_next_link_malformed() {
        assert_eq!(parse_next_link("garbage", &base()), None);
        assert_eq!(parse_next_link("<not a url>; rel=\"next\"", &base()), None);
    }
}
