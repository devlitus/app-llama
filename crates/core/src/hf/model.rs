//! Tipos de dominio de las respuestas de la API de Hugging Face.
use serde::{Deserialize, Deserializer, Serialize};

/// Acceso restringido de un repositorio: HF publica `false`, `"auto"` o `"manual"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Gated {
    Open,
    Auto,
    Manual,
}

impl<'de> Deserialize<'de> for Gated {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Raw {
            Flag(bool),
            Mode(String),
        }
        match Raw::deserialize(deserializer)? {
            Raw::Flag(false) => Ok(Gated::Open),
            Raw::Mode(mode) if mode == "auto" => Ok(Gated::Auto),
            Raw::Mode(mode) if mode == "manual" => Ok(Gated::Manual),
            Raw::Flag(true) | Raw::Mode(_) => {
                Err(serde::de::Error::custom("valor de `gated` desconocido"))
            }
        }
    }
}

/// Metadatos GGUF que HF publica tanto en el listado como en la ficha.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GgufInfo {
    pub total: Option<u64>,
    pub architecture: Option<String>,
    pub context_length: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSummary {
    pub id: String,
    pub downloads: u64,
    pub likes: u64,
    pub gated: Gated,
    #[serde(rename(deserialize = "lastModified"))]
    pub last_modified: Option<String>,
    pub pipeline_tag: Option<String>,
    pub gguf: Option<GgufInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "RawModelInfo")]
pub struct ModelInfo {
    pub id: String,
    pub gated: Gated,
    pub downloads: u64,
    pub likes: u64,
    pub gguf: Option<GgufInfo>,
    pub license: Option<String>,
    /// Commit actual de la rama principal, si HF lo publica.
    pub sha: Option<String>,
}

#[derive(Deserialize)]
struct RawModelInfo {
    id: String,
    gated: Gated,
    downloads: u64,
    likes: u64,
    gguf: Option<GgufInfo>,
    #[serde(rename = "cardData")]
    card_data: Option<RawCardData>,
    sha: Option<String>,
}

#[derive(Deserialize)]
struct RawCardData {
    license: Option<String>,
}

impl From<RawModelInfo> for ModelInfo {
    fn from(raw: RawModelInfo) -> Self {
        ModelInfo {
            id: raw.id,
            gated: raw.gated,
            downloads: raw.downloads,
            likes: raw.likes,
            gguf: raw.gguf,
            license: raw.card_data.and_then(|card| card.license),
            sha: raw.sha,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TreeEntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LfsInfo {
    pub oid: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TreeEntry {
    #[serde(rename = "type")]
    pub kind: TreeEntryKind,
    pub path: String,
    pub size: u64,
    pub lfs: Option<LfsInfo>,
}

/// Usuario autenticado; `role` es el rol del token de acceso, si lo hay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "RawWhoAmI")]
pub struct WhoAmI {
    pub name: String,
    pub role: Option<String>,
}

#[derive(Deserialize)]
struct RawWhoAmI {
    name: String,
    auth: Option<RawAuth>,
}

#[derive(Deserialize)]
struct RawAuth {
    #[serde(rename = "accessToken")]
    access_token: Option<RawAccessToken>,
}

#[derive(Deserialize)]
struct RawAccessToken {
    role: Option<String>,
}

impl From<RawWhoAmI> for WhoAmI {
    fn from(raw: RawWhoAmI) -> Self {
        WhoAmI {
            name: raw.name,
            role: raw
                .auth
                .and_then(|auth| auth.access_token)
                .and_then(|token| token.role),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const SEARCH_PAGE: &str = include_str!("../../fixtures/hf/search_page.json");
    const INFO_GGUF: &str = include_str!("../../fixtures/hf/model_info_gguf.json");
    const INFO_NO_GGUF: &str = include_str!("../../fixtures/hf/model_info_no_gguf.json");
    const INFO_GATED: &str = include_str!("../../fixtures/hf/model_info_gated_manual.json");
    const TREE: &str = include_str!("../../fixtures/hf/tree_unsloth.json");
    const WHOAMI: &str = include_str!("../../fixtures/hf/whoami.json");

    fn search_items() -> Vec<ModelSummary> {
        serde_json::from_str(SEARCH_PAGE).unwrap()
    }

    fn first_search_item_json() -> Value {
        let page: Vec<Value> = serde_json::from_str(SEARCH_PAGE).unwrap();
        page[0].clone()
    }

    #[test]
    fn parse_search_page() {
        let items = search_items();
        assert_eq!(items.len(), 3);
        let first = &items[0];
        assert_eq!(first.id, "bartowski/Meta-Llama-3.1-8B-Instruct-GGUF");
        assert_eq!(first.downloads, 371263);
        assert_eq!(first.likes, 437);
        let gguf = first.gguf.as_ref().unwrap();
        assert_eq!(gguf.total, Some(8030261312));
        assert_eq!(gguf.context_length, Some(131072));
        assert!(first.last_modified.is_some());
    }

    #[test]
    fn parse_search_missing_pipeline_tag() {
        let mut item = first_search_item_json();
        item.as_object_mut().unwrap().remove("pipeline_tag");
        let parsed: ModelSummary = serde_json::from_value(item).unwrap();
        assert_eq!(parsed.pipeline_tag, None);
    }

    #[test]
    fn parse_search_item_without_gguf() {
        let mut item = first_search_item_json();
        item.as_object_mut().unwrap().remove("gguf");
        let parsed: ModelSummary = serde_json::from_value(item).unwrap();
        assert_eq!(parsed.gguf, None);
    }

    #[test]
    fn parse_search_gguf_without_total() {
        let mut item = first_search_item_json();
        item["gguf"] = json!({ "architecture": "llama" });
        let parsed: ModelSummary = serde_json::from_value(item).unwrap();
        assert_eq!(parsed.gguf.unwrap().total, None);
    }

    #[test]
    fn parse_gated_false() {
        assert_eq!(serde_json::from_str::<Gated>("false").unwrap(), Gated::Open);
    }

    #[test]
    fn parse_gated_auto() {
        assert_eq!(
            serde_json::from_str::<Gated>("\"auto\"").unwrap(),
            Gated::Auto
        );
    }

    #[test]
    fn parse_gated_manual() {
        let info: ModelInfo = serde_json::from_str(INFO_GATED).unwrap();
        assert_eq!(info.gated, Gated::Manual);
    }

    #[test]
    fn parse_gated_unknown_value_fails() {
        assert!(serde_json::from_str::<Gated>("\"other\"").is_err());
        assert!(serde_json::from_str::<Gated>("true").is_err());
    }

    #[test]
    fn parse_model_info_with_gguf() {
        let info: ModelInfo = serde_json::from_str(INFO_GGUF).unwrap();
        let gguf = info.gguf.unwrap();
        assert_eq!(gguf.total, Some(27320697856));
        assert_eq!(gguf.architecture.as_deref(), Some("qwen35"));
        assert!(gguf.context_length.unwrap() >= 8192);
        assert_eq!(info.license.as_deref(), Some("apache-2.0"));
    }

    #[test]
    fn parse_model_info_con_sha() {
        let mut value: Value = serde_json::from_str(INFO_GGUF).unwrap();
        value["sha"] = json!("9217f5db79a29953eb74d5343926648285ec7e67");
        let info: ModelInfo = serde_json::from_value(value).unwrap();
        assert_eq!(
            info.sha.as_deref(),
            Some("9217f5db79a29953eb74d5343926648285ec7e67")
        );
    }

    #[test]
    fn parse_model_info_sin_sha() {
        let info: ModelInfo = serde_json::from_str(INFO_GGUF).unwrap();
        assert_eq!(info.sha, None);
    }

    #[test]
    fn parse_model_info_without_gguf() {
        let info: ModelInfo = serde_json::from_str(INFO_NO_GGUF).unwrap();
        assert_eq!(info.gguf, None);
    }

    #[test]
    fn parse_tree_entries() {
        let entries: Vec<TreeEntry> = serde_json::from_str(TREE).unwrap();
        assert_eq!(entries.len(), 13);
        let bf16 = entries.iter().find(|e| e.path == "BF16").unwrap();
        assert_eq!(bf16.kind, TreeEntryKind::Directory);
        let q4 = entries
            .iter()
            .find(|e| e.path == "Qwen3.8-27B-Q4_0.gguf")
            .unwrap();
        assert_eq!(q4.kind, TreeEntryKind::File);
        let lfs = q4.lfs.as_ref().unwrap();
        assert_eq!(lfs.oid.len(), 64);
        assert!(lfs.oid.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(lfs.size, 16_056_478_688);
    }

    #[test]
    fn parse_tree_non_lfs_file() {
        let entries: Vec<TreeEntry> = serde_json::from_str(TREE).unwrap();
        let config = entries.iter().find(|e| e.path == "config.json").unwrap();
        assert_eq!(config.lfs, None);
        assert_eq!(config.size, 3760);
    }

    #[test]
    fn parse_whoami() {
        let who: WhoAmI = serde_json::from_str(WHOAMI).unwrap();
        assert_eq!(who.name, "ejemplo");
        assert_eq!(who.role.as_deref(), Some("read"));
    }

    #[test]
    fn parse_whoami_without_access_token() {
        let who: WhoAmI =
            serde_json::from_value(json!({ "name": "ejemplo", "auth": { "type": "oauth" } }))
                .unwrap();
        assert_eq!(who.role, None);
    }

    #[test]
    fn parse_ignores_unknown_fields() {
        let mut item = first_search_item_json();
        item["foo"] = json!("bar");
        assert!(serde_json::from_value::<ModelSummary>(item).is_ok());
    }
}
