//! Modelo de las releases de GitHub de llama.cpp y elección de la última build.
use serde::Deserialize;

use crate::runtime::RuntimeError;

const SHA256_PREFIX: &str = "sha256:";
const SHA256_HEX_LEN: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Asset {
    pub name: String,
    pub size: u64,
    pub digest: Option<String>,
    pub browser_download_url: String,
}

impl Asset {
    /// Hash SHA256 publicado por GitHub; `None` si falta o no tiene el formato `sha256:<64 hex>`.
    pub fn sha256(&self) -> Option<[u8; 32]> {
        let hex = self.digest.as_deref()?.strip_prefix(SHA256_PREFIX)?;
        decode_sha256_hex(hex)
    }
}

fn decode_sha256_hex(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != SHA256_HEX_LEN || !hex.is_ascii() {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (byte, pair) in bytes.iter_mut().zip(hex.as_bytes().chunks(2)) {
        let pair = std::str::from_utf8(pair).ok()?;
        *byte = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(bytes)
}

pub fn parse_releases(json: &str) -> Result<Vec<Release>, RuntimeError> {
    serde_json::from_str(json).map_err(RuntimeError::ReleaseParse)
}

/// Número de build de un tag `bNNNN`; `None` para cualquier otro formato.
pub fn build_number(tag: &str) -> Option<u32> {
    let digits = tag.strip_prefix('b')?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// La release de mayor número de build que no es borrador y cumple `is_compatible`.
/// `prerelease` no cuenta: las builds reales de llama.cpp se publican como prerelease.
pub fn latest_build_release(
    releases: &[Release],
    is_compatible: impl Fn(&Release) -> bool,
) -> Result<&Release, RuntimeError> {
    releases
        .iter()
        .filter(|release| !release.draft && is_compatible(release))
        .filter_map(|release| Some((build_number(&release.tag_name)?, release)))
        .max_by_key(|(build, _)| *build)
        .map(|(_, release)| release)
        .ok_or(RuntimeError::NoCompatibleRelease)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::RuntimeError;

    const FIXTURE: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/releases_b12780.json"
    ));

    fn release(tag: &str, draft: bool, asset_names: &[&str]) -> Release {
        Release {
            tag_name: tag.to_string(),
            draft,
            prerelease: true,
            assets: asset_names
                .iter()
                .map(|name| Asset {
                    name: (*name).to_string(),
                    size: 1,
                    digest: None,
                    browser_download_url: format!("https://example.test/{name}"),
                })
                .collect(),
        }
    }

    fn tags(releases: &[Release]) -> Vec<&str> {
        releases.iter().map(|r| r.tag_name.as_str()).collect()
    }

    #[test]
    fn parse_releases_fixture_real() {
        let releases = parse_releases(FIXTURE).unwrap();
        assert_eq!(tags(&releases), ["v1.1.0", "b12780", "b12779"]);
        let b12780 = &releases[1];
        assert!(b12780.prerelease);
        let cpu = b12780
            .assets
            .iter()
            .find(|a| a.name == "llama-b12780-bin-win-cpu-x64.zip")
            .unwrap();
        assert!(cpu.size > 0);
        assert!(cpu.sha256().is_some());
    }

    #[test]
    fn parse_releases_json_invalido() {
        assert!(matches!(
            parse_releases("{no json"),
            Err(RuntimeError::ReleaseParse(_))
        ));
    }

    #[test]
    fn build_number_tags() {
        assert_eq!(build_number("b12780"), Some(12780));
        assert_eq!(build_number("b1"), Some(1));
        assert_eq!(build_number("v1.1.0"), None);
        assert_eq!(build_number("b12a"), None);
        assert_eq!(build_number("B12"), None);
        assert_eq!(build_number(""), None);
    }

    fn asset_with_digest(digest: Option<&str>) -> Asset {
        Asset {
            name: "a.zip".to_string(),
            size: 1,
            digest: digest.map(str::to_string),
            browser_download_url: "https://example.test/a.zip".to_string(),
        }
    }

    #[test]
    fn sha256_digest_valido() {
        let hex = "00ff".repeat(16);
        let asset = asset_with_digest(Some(&format!("sha256:{hex}")));
        let expected: [u8; 32] = std::array::from_fn(|i| if i % 2 == 0 { 0x00 } else { 0xff });
        assert_eq!(asset.sha256(), Some(expected));
    }

    #[test]
    fn sha256_digest_invalido_o_ausente() {
        let sixty_four_a = "a".repeat(64);
        let sixty_four_z = "z".repeat(64);
        let invalid = [
            None,
            Some(format!("md5:{sixty_four_a}")),
            Some("sha256:abc".to_string()),
            Some(format!("sha256:{sixty_four_z}")),
        ];
        for digest in invalid {
            assert_eq!(asset_with_digest(digest.as_deref()).sha256(), None);
        }
    }

    #[test]
    fn ultima_build_ignora_v_y_draft() {
        let releases = [
            release("v1.1.0", false, &[]),
            release("b12781", true, &[]),
            release("b12780", false, &[]),
            release("b12779", false, &[]),
        ];
        let latest = latest_build_release(&releases, |_| true).unwrap();
        assert_eq!(latest.tag_name, "b12780");
    }

    #[test]
    fn ultima_build_respeta_predicado() {
        let releases = [
            release("b12780", false, &[]),
            release("b12779", false, &["llama-b12779-bin-win-cpu-x64.zip"]),
        ];
        let latest = latest_build_release(&releases, |r| !r.assets.is_empty()).unwrap();
        assert_eq!(latest.tag_name, "b12779");
    }

    #[test]
    fn ultima_build_no_depende_del_orden() {
        let releases = [release("b12779", false, &[]), release("b12780", false, &[])];
        let latest = latest_build_release(&releases, |_| true).unwrap();
        assert_eq!(latest.tag_name, "b12780");
    }

    #[test]
    fn ultima_build_sin_compatibles() {
        let releases = [release("v1.1.0", false, &[])];
        assert!(matches!(
            latest_build_release(&releases, |_| true),
            Err(RuntimeError::NoCompatibleRelease)
        ));
    }
}
