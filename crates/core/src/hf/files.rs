//! Filtrado de los archivos GGUF de un repositorio y agrupación de los modelos divididos.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::model::{TreeEntry, TreeEntryKind};
use super::quant::{GGUF_EXTENSION, extract_quant, parse_valid_split_suffix, strip_gguf_extension};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GgufPart {
    pub path: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GgufVariant {
    pub name: String,
    pub quant: Option<String>,
    pub parts: Vec<GgufPart>,
    pub total_bytes: u64,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadRequest {
    pub repo_id: String,
    pub revision: String,
    pub parts: Vec<GgufPart>,
}

/// Un único archivo se agrupa por su ruta; las partes de un split, por
/// carpeta, prefijo y número total de partes.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum GroupKey {
    Single(String),
    Split { prefix: String, total: u32 },
}

struct Candidate {
    key: GroupKey,
    index: u32,
    total: u32,
    name: String,
    part: GgufPart,
}

pub fn group_gguf_files(entries: &[TreeEntry]) -> Vec<GgufVariant> {
    let mut groups: BTreeMap<GroupKey, Vec<Candidate>> = BTreeMap::new();
    for candidate in entries.iter().filter_map(candidate_from) {
        groups
            .entry(candidate.key.clone())
            .or_default()
            .push(candidate);
    }
    let mut variants: Vec<GgufVariant> = groups.into_values().map(build_variant).collect();
    variants.sort_by(|a, b| (a.total_bytes, &a.name).cmp(&(b.total_bytes, &b.name)));
    variants
}

fn candidate_from(entry: &TreeEntry) -> Option<Candidate> {
    if entry.kind != TreeEntryKind::File || !is_model_file_name(base_name(&entry.path)) {
        return None;
    }
    let stem = strip_gguf_extension(&entry.path)?;
    let part = GgufPart {
        path: entry.path.clone(),
        size: entry.lfs.as_ref().map_or(entry.size, |lfs| lfs.size),
        sha256: entry.lfs.as_ref().map(|lfs| lfs.oid.clone()),
    };
    let split = parse_valid_split_suffix(stem);
    Some(match split {
        Some(split) => {
            let prefix = stem[..split.prefix_len].to_owned();
            Candidate {
                key: GroupKey::Split {
                    prefix: prefix.clone(),
                    total: split.total,
                },
                index: split.index,
                total: split.total,
                name: prefix,
                part,
            }
        }
        None => Candidate {
            key: GroupKey::Single(entry.path.clone()),
            index: 1,
            total: 1,
            name: stem.to_owned(),
            part,
        },
    })
}

fn base_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Excluye lo que no es un modelo de texto: proyectores multimodales,
/// matrices de importancia y borradores MTP.
fn is_model_file_name(base_name: &str) -> bool {
    let lower = base_name.to_ascii_lowercase();
    lower.ends_with(GGUF_EXTENSION)
        && !lower.starts_with("mmproj")
        && !lower.starts_with("mtp")
        && !lower.contains("imatrix")
}

fn build_variant(mut group: Vec<Candidate>) -> GgufVariant {
    group.sort_by_key(|candidate| candidate.index);
    let total = group[0].total;
    let complete = group.iter().map(|candidate| candidate.index).eq(1..=total);
    let name = group[0].name.clone();
    let quant = extract_quant(&group[0].part.path);
    let parts: Vec<GgufPart> = group.into_iter().map(|candidate| candidate.part).collect();
    let total_bytes = parts
        .iter()
        .fold(0, |sum, part| u64::saturating_add(sum, part.size));
    GgufVariant {
        name,
        quant,
        parts,
        total_bytes,
        complete,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hf::model::{LfsInfo, TreeEntryKind};

    const TREE: &str = include_str!("../../fixtures/hf/tree_unsloth.json");
    const BF16_PART_1: &str = "BF16/Qwen3.8-27B-BF16-00001-of-00002.gguf";
    const BF16_PART_2: &str = "BF16/Qwen3.8-27B-BF16-00002-of-00002.gguf";

    fn file(path: &str, size: u64) -> TreeEntry {
        TreeEntry {
            kind: TreeEntryKind::File,
            path: path.to_owned(),
            size,
            lfs: None,
        }
    }

    fn lfs_file(path: &str, size: u64, oid: &str) -> TreeEntry {
        TreeEntry {
            lfs: Some(LfsInfo {
                oid: oid.to_owned(),
                size,
            }),
            ..file(path, 136)
        }
    }

    fn tree() -> Vec<TreeEntry> {
        serde_json::from_str(TREE).unwrap()
    }

    fn bf16_entries_reversed() -> Vec<TreeEntry> {
        tree()
            .into_iter()
            .filter(|e| e.path.starts_with("BF16/"))
            .rev()
            .collect()
    }

    fn names(variants: &[GgufVariant]) -> Vec<&str> {
        variants.iter().map(|v| v.name.as_str()).collect()
    }

    #[test]
    fn group_single_file() {
        let variants = group_gguf_files(&[file("a-Q4_0.gguf", 100)]);
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].parts.len(), 1);
        assert_eq!(variants[0].total_bytes, 100);
        assert!(variants[0].complete);
        assert_eq!(variants[0].quant.as_deref(), Some("Q4_0"));
    }

    #[test]
    fn group_split_two_parts() {
        let variants = group_gguf_files(&bf16_entries_reversed());
        assert_eq!(variants.len(), 1);
        let variant = &variants[0];
        assert_eq!(variant.quant.as_deref(), Some("BF16"));
        let paths: Vec<&str> = variant.parts.iter().map(|p| p.path.as_str()).collect();
        assert_eq!(paths, [BF16_PART_1, BF16_PART_2]);
        assert_eq!(variant.total_bytes, 54_657_735_616);
        assert!(variant.complete);
        assert_eq!(variant.name, "BF16/Qwen3.8-27B-BF16");
    }

    #[test]
    fn group_split_missing_part_incomplete() {
        let variants = group_gguf_files(&[
            file("m-00001-of-00003.gguf", 10),
            file("m-00003-of-00003.gguf", 30),
        ]);
        assert_eq!(variants.len(), 1);
        assert!(!variants[0].complete);
        assert_eq!(variants[0].total_bytes, 40);
    }

    #[test]
    fn group_split_duplicate_part_incomplete() {
        let variants = group_gguf_files(&[
            file("m-00001-of-00002.gguf", 10),
            file("m-00001-of-00002.gguf", 10),
        ]);
        assert_eq!(variants.len(), 1);
        assert!(!variants[0].complete);
    }

    #[test]
    fn group_split_index_zero_is_single() {
        let variants = group_gguf_files(&[file("x-00000-of-00002.gguf", 5)]);
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].parts.len(), 1);
        assert!(variants[0].complete);
    }

    #[test]
    fn group_split_index_above_total_is_single() {
        let variants = group_gguf_files(&[file("x-00003-of-00002.gguf", 5)]);
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].parts.len(), 1);
        assert!(variants[0].complete);
    }

    #[test]
    fn group_split_total_zero_is_single() {
        let variants = group_gguf_files(&[file("x-00001-of-00000.gguf", 5)]);
        assert_eq!(variants.len(), 1);
        assert!(variants[0].complete);
    }

    #[test]
    fn group_excludes_mmproj_and_imatrix() {
        let variants = group_gguf_files(&tree());
        assert!(variants.iter().all(|v| !v.name.contains("mmproj")));
        assert!(variants.iter().all(|v| !v.name.contains("imatrix")));
        assert!(
            variants
                .iter()
                .flat_map(|v| &v.parts)
                .all(|p| !p.path.contains("mmproj") && !p.path.contains("imatrix"))
        );
    }

    #[test]
    fn group_excludes_mmproj_and_imatrix_inline() {
        let variants = group_gguf_files(&[
            file("sub/MMPROJ-model-f16.gguf", 1),
            file("Model-IMatrix-Q4_0.gguf", 1),
            file("imatrix.gguf", 1),
        ]);
        assert!(variants.is_empty());
    }

    #[test]
    fn group_excludes_mtp() {
        let variants = group_gguf_files(&tree());
        assert_eq!(
            names(&variants),
            [
                "Qwen3.8-27B-UD-IQ2_XXS",
                "Qwen3.8-27B-Q4_0",
                "Qwen3.8-27B-UD-Q4_K_XL",
                "BF16/Qwen3.8-27B-BF16"
            ]
        );
        let quants: Vec<_> = variants.iter().filter_map(|v| v.quant.as_deref()).collect();
        assert_eq!(quants, ["IQ2_XXS", "Q4_0", "Q4_K_XL", "BF16"]);
        assert!(
            variants
                .iter()
                .flat_map(|v| &v.parts)
                .all(|p| !p.path.starts_with("MTP/"))
        );

        let inline = group_gguf_files(&[
            file("MTP/mtp-Qwen3.8-27B-Q4_0.gguf", 1),
            file("MTP-x-Q8_0.gguf", 1),
            file("drafts/Mtp_y.gguf", 1),
        ]);
        assert!(inline.is_empty());
    }

    #[test]
    fn group_keeps_mtp_not_at_start() {
        let variants = group_gguf_files(&[file("Qwen3-MTP-Q4_0.gguf", 1)]);
        assert_eq!(variants.len(), 1);
        assert_eq!(variants[0].quant.as_deref(), Some("Q4_0"));
    }

    #[test]
    fn group_excludes_directories_and_non_gguf() {
        let directory = TreeEntry {
            kind: TreeEntryKind::Directory,
            ..file("BF16", 0)
        };
        let variants =
            group_gguf_files(&[directory, file("README.md", 10), file("config.json", 20)]);
        assert!(variants.is_empty());
    }

    #[test]
    fn group_excludes_directory_named_like_gguf() {
        let directory = TreeEntry {
            kind: TreeEntryKind::Directory,
            ..file("weird-Q4_0.gguf", 0)
        };
        assert!(group_gguf_files(&[directory]).is_empty());
    }

    #[test]
    fn group_extension_case_insensitive() {
        let variants = group_gguf_files(&[file("model-Q8_0.GGUF", 1)]);
        assert_eq!(variants.len(), 1);
    }

    #[test]
    fn group_prefers_lfs_size() {
        let variants = group_gguf_files(&[lfs_file("m-Q4_0.gguf", 1000, "abc")]);
        assert_eq!(variants[0].total_bytes, 1000);
        assert_eq!(variants[0].parts[0].size, 1000);
    }

    #[test]
    fn group_same_prefix_different_dirs() {
        let variants = group_gguf_files(&[
            file("a/m-00001-of-00001.gguf", 1),
            file("b/m-00001-of-00001.gguf", 1),
        ]);
        assert_eq!(variants.len(), 2);
    }

    #[test]
    fn group_sorted_by_size_then_name() {
        let variants = group_gguf_files(&[
            file("b.gguf", 300),
            file("c.gguf", 100),
            file("a.gguf", 100),
        ]);
        assert_eq!(names(&variants), ["a", "c", "b"]);
    }

    #[test]
    fn group_empty_tree() {
        assert!(group_gguf_files(&[]).is_empty());
    }

    #[test]
    fn download_request_deserializa() {
        let request: DownloadRequest = serde_json::from_str(
            r#"{"repo_id":"a/b","revision":"main","parts":[{"path":"m.gguf","size":10,"sha256":null}]}"#,
        )
        .unwrap();
        assert_eq!(
            request,
            DownloadRequest {
                repo_id: "a/b".to_owned(),
                revision: "main".to_owned(),
                parts: vec![GgufPart {
                    path: "m.gguf".to_owned(),
                    size: 10,
                    sha256: None,
                }],
            }
        );
    }
}
