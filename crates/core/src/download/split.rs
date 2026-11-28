//! Validación de las partes que forman una unidad de descarga.
use super::error::DownloadError;
use crate::hf::quant::{SplitSuffix, parse_valid_split_suffix, strip_gguf_extension};

/// Un fichero único vale solo; las partes de un split deben compartir carpeta,
/// prefijo y total, y cubrir exactamente `1..=total` sin repetir.
pub fn validate_parts(paths: &[&str]) -> Result<(), DownloadError> {
    if paths.is_empty() {
        return Err(DownloadError::InvalidRequest(
            "no hay ficheros que descargar",
        ));
    }
    let parts = paths
        .iter()
        .map(|path| split_part(path))
        .collect::<Result<Vec<_>, _>>()?;
    match parts.as_slice() {
        [None] => Ok(()),
        [Some((prefix, first)), ..] => validate_split(&parts, prefix, first.total),
        _ => Err(DownloadError::IncompleteSplit),
    }
}

type SplitPart<'a> = Option<(&'a str, SplitSuffix)>;

fn split_part(path: &str) -> Result<SplitPart<'_>, DownloadError> {
    let stem = strip_gguf_extension(path)
        .ok_or(DownloadError::InvalidRequest("el fichero no es un .gguf"))?;
    Ok(parse_valid_split_suffix(stem).map(|split| (&stem[..split.prefix_len], split)))
}

fn validate_split(parts: &[SplitPart<'_>], prefix: &str, total: u32) -> Result<(), DownloadError> {
    let mut indices = Vec::with_capacity(parts.len());
    for part in parts {
        match part {
            Some((other, split)) if *other == prefix && split.total == total => {
                indices.push(split.index);
            }
            _ => return Err(DownloadError::IncompleteSplit),
        }
    }
    indices.sort_unstable();
    if indices.into_iter().eq(1..=total) {
        Ok(())
    } else {
        Err(DownloadError::IncompleteSplit)
    }
}

/// La ruta sin sufijo de split ni extensión (igual que `GgufVariant::name`).
pub fn display_name(first_path: &str) -> String {
    let stem = strip_gguf_extension(first_path).unwrap_or(first_path);
    match parse_valid_split_suffix(stem) {
        Some(split) => stem[..split.prefix_len].to_owned(),
        None => stem.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(prefix: &str, index: u32, total: u32) -> String {
        format!("{prefix}-{index:05}-of-{total:05}.gguf")
    }

    fn validate(paths: &[String]) -> Result<(), DownloadError> {
        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        validate_parts(&refs)
    }

    #[test]
    fn valida_fichero_unico() {
        assert!(validate_parts(&["m-Q4_K_M.gguf"]).is_ok());
    }

    #[test]
    fn valida_split_completo() {
        let paths = [part("m", 1, 3), part("m", 2, 3), part("m", 3, 3)];
        assert!(validate(&paths).is_ok());
    }

    #[test]
    fn valida_split_desordenado() {
        let paths = [part("m", 3, 3), part("m", 1, 3), part("m", 2, 3)];
        assert!(validate(&paths).is_ok());
    }

    #[test]
    fn rechaza_parte_que_falta() {
        let paths = [part("m", 1, 3), part("m", 3, 3)];
        assert!(matches!(
            validate(&paths),
            Err(DownloadError::IncompleteSplit)
        ));
    }

    #[test]
    fn rechaza_duplicados() {
        let paths = [part("m", 1, 2), part("m", 1, 2), part("m", 2, 2)];
        assert!(matches!(
            validate(&paths),
            Err(DownloadError::IncompleteSplit)
        ));
    }

    #[test]
    fn rechaza_prefijos_mezclados() {
        let paths = [part("a", 1, 2), part("b", 2, 2)];
        assert!(matches!(
            validate(&paths),
            Err(DownloadError::IncompleteSplit)
        ));
    }

    #[test]
    fn rechaza_carpetas_mezcladas() {
        let paths = [part("x/m", 1, 2), part("y/m", 2, 2)];
        assert!(matches!(
            validate(&paths),
            Err(DownloadError::IncompleteSplit)
        ));
    }

    #[test]
    fn rechaza_totales_mezclados() {
        let paths = [part("m", 1, 2), part("m", 2, 3)];
        assert!(matches!(
            validate(&paths),
            Err(DownloadError::IncompleteSplit)
        ));
    }

    #[test]
    fn rechaza_unico_mas_split() {
        let paths = ["m.gguf".to_owned(), part("m", 1, 1)];
        assert!(matches!(
            validate(&paths),
            Err(DownloadError::IncompleteSplit)
        ));
    }

    #[test]
    fn rechaza_lista_vacia() {
        assert!(matches!(
            validate_parts(&[]),
            Err(DownloadError::InvalidRequest(_))
        ));
    }

    #[test]
    fn rechaza_no_gguf() {
        assert!(matches!(
            validate_parts(&["README.md"]),
            Err(DownloadError::InvalidRequest(_))
        ));
    }

    #[test]
    fn display_name_split_en_carpeta() {
        assert_eq!(
            display_name("BF16/Qwen3.8-27B-BF16-00001-of-00002.gguf"),
            "BF16/Qwen3.8-27B-BF16"
        );
    }

    #[test]
    fn valida_indice_cero_como_unico() {
        assert!(validate_parts(&["x-00000-of-00002.gguf"]).is_ok());
    }

    #[test]
    fn valida_indice_mayor_que_total_como_unico() {
        assert!(validate_parts(&["x-00003-of-00002.gguf"]).is_ok());
    }

    #[test]
    fn rechaza_split_sin_prefijo() {
        assert!(matches!(
            validate_parts(&["-00001-of-00002.gguf", "-00002-of-00002.gguf"]),
            Err(DownloadError::IncompleteSplit)
        ));
    }

    #[test]
    fn rechaza_dos_unicos_distintos() {
        assert!(matches!(
            validate_parts(&["a.gguf", "b.gguf"]),
            Err(DownloadError::IncompleteSplit)
        ));
    }

    #[test]
    fn display_name_indice_invalido() {
        assert_eq!(display_name("x-00000-of-00002.gguf"), "x-00000-of-00002");
    }

    #[test]
    fn display_name_unico() {
        assert_eq!(display_name("m-Q4_0.gguf"), "m-Q4_0");
    }
}
