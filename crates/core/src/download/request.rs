//! Petición de alta de una unidad de descarga.
use crate::hf::{DownloadRequest, GgufPart, RepoId, is_valid_revision};

use super::error::DownloadError;

const SHA256_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartRequest {
    pub relative_path: String,
    pub size: u64,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitRequest {
    pub repo: RepoId,
    pub revision: String,
    pub parts: Vec<PartRequest>,
}

impl TryFrom<DownloadRequest> for UnitRequest {
    type Error = DownloadError;

    /// La petición viene de la UI: se valida todo lo que luego formará rutas,
    /// URLs y comparaciones de hash.
    fn try_from(request: DownloadRequest) -> Result<Self, Self::Error> {
        let repo = RepoId::parse(&request.repo_id)
            .map_err(|_| DownloadError::InvalidRequest("repositorio no válido"))?;
        if !is_valid_revision(&request.revision) {
            return Err(DownloadError::InvalidRequest("revisión no válida"));
        }
        let parts = request
            .parts
            .into_iter()
            .map(PartRequest::try_from)
            .collect::<Result<_, _>>()?;
        Ok(UnitRequest {
            repo,
            revision: request.revision,
            parts,
        })
    }
}

impl TryFrom<GgufPart> for PartRequest {
    type Error = DownloadError;

    fn try_from(part: GgufPart) -> Result<Self, Self::Error> {
        if part.size == 0 {
            return Err(DownloadError::InvalidRequest("tamaño de parte no válido"));
        }
        if part.sha256.as_deref().is_some_and(|sha| !is_sha256(sha)) {
            return Err(DownloadError::InvalidRequest("sha256 no válido"));
        }
        Ok(PartRequest {
            relative_path: part.path,
            size: part.size,
            sha256: part.sha256,
        })
    }
}

fn is_sha256(hex_digest: &str) -> bool {
    let mut digest = [0u8; SHA256_LEN];
    hex::decode_to_slice(hex_digest, &mut digest).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn part(path: &str, size: u64, sha256: Option<&str>) -> GgufPart {
        GgufPart {
            path: path.to_owned(),
            size,
            sha256: sha256.map(str::to_owned),
        }
    }

    fn request(parts: Vec<GgufPart>) -> DownloadRequest {
        DownloadRequest {
            repo_id: "a/b".to_owned(),
            revision: "main".to_owned(),
            parts,
        }
    }

    fn single(size: u64, sha256: Option<&str>) -> DownloadRequest {
        request(vec![part("m.gguf", size, sha256)])
    }

    fn assert_invalid(request: DownloadRequest) {
        assert!(matches!(
            UnitRequest::try_from(request),
            Err(DownloadError::InvalidRequest(_))
        ));
    }

    #[test]
    fn unit_request_valido() {
        let unit = UnitRequest::try_from(request(vec![
            part("m-00001-of-00002.gguf", 10, Some(SHA_A)),
            part("m-00002-of-00002.gguf", 20, Some(SHA_A)),
        ]))
        .unwrap();
        assert_eq!(unit.repo.as_str(), "a/b");
        assert_eq!(unit.revision, "main");
        let paths: Vec<&str> = unit
            .parts
            .iter()
            .map(|p| p.relative_path.as_str())
            .collect();
        assert_eq!(paths, ["m-00001-of-00002.gguf", "m-00002-of-00002.gguf"]);
        assert_eq!(unit.parts[1].size, 20);
        assert_eq!(unit.parts[0].sha256.as_deref(), Some(SHA_A));
    }

    #[test]
    fn unit_request_repo_invalido() {
        let mut bad = single(10, None);
        bad.repo_id = "bad id".to_owned();
        assert_invalid(bad);
    }

    #[test]
    fn unit_request_revision_invalida() {
        let mut bad = single(10, None);
        bad.revision = "../x".to_owned();
        assert_invalid(bad);
    }

    #[test]
    fn unit_request_sha_laxo() {
        for sha in ["+f".repeat(32), "a".repeat(63), "a".repeat(65)] {
            assert_invalid(single(10, Some(&sha)));
        }
    }

    #[test]
    fn unit_request_tamano_cero() {
        assert_invalid(single(0, None));
    }
}
