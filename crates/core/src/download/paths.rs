use std::path::{Path, PathBuf};

use super::error::DownloadError;

/// Une `relative` (una ruta de HF, siempre con `/`) a `base` rechazando todo
/// lo que pueda escapar de `base`.
pub fn safe_join(base: &Path, relative: &str) -> Result<PathBuf, DownloadError> {
    if relative.is_empty() || relative.contains('\\') {
        return Err(DownloadError::UnsafePath);
    }
    relative
        .split('/')
        .try_fold(base.to_path_buf(), |path, component| {
            if is_safe_component(component) {
                Ok(path.join(component))
            } else {
                Err(DownloadError::UnsafePath)
            }
        })
}

// Los dos puntos cubren los prefijos de unidad y los flujos alternativos de NTFS.
fn is_safe_component(component: &str) -> bool {
    !matches!(component, "" | "." | "..") && !component.contains(':')
}

/// Ruta del fichero parcial: añade `.part` al nombre completo.
pub fn part_path(final_path: &Path) -> PathBuf {
    let mut name = final_path.as_os_str().to_os_string();
    name.push(".part");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unsafe_path(relative: &str) -> bool {
        matches!(
            safe_join(Path::new("base"), relative),
            Err(DownloadError::UnsafePath)
        )
    }

    #[test]
    fn safe_join_ruta_simple() {
        let tmp = tempfile::TempDir::new().unwrap();
        let joined = safe_join(tmp.path(), "a/b.gguf").unwrap();
        assert_eq!(joined, tmp.path().join("a").join("b.gguf"));
    }

    #[test]
    fn safe_join_rechaza_padre() {
        assert!(unsafe_path("../x.gguf"));
    }

    #[test]
    fn safe_join_rechaza_padre_intermedio() {
        assert!(unsafe_path("a/../../x"));
    }

    #[test]
    fn safe_join_rechaza_punto() {
        assert!(unsafe_path("./x.gguf"));
        assert!(unsafe_path("a/./x.gguf"));
    }

    #[test]
    fn safe_join_rechaza_absoluta() {
        assert!(unsafe_path("/etc/x"));
        assert!(unsafe_path("C:\\x"));
        assert!(unsafe_path("C:x"));
    }

    #[test]
    fn safe_join_rechaza_barra_invertida() {
        assert!(unsafe_path("a\\b.gguf"));
    }

    #[test]
    fn safe_join_rechaza_vacia() {
        assert!(unsafe_path(""));
        assert!(unsafe_path("a//b"));
    }

    #[test]
    fn part_path_anade_sufijo() {
        assert_eq!(
            part_path(Path::new("dir").join("m.gguf").as_path()),
            Path::new("dir").join("m.gguf.part")
        );
    }
}
