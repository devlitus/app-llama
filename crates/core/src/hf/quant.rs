//! Extracción de la cuantización a partir de la ruta de un archivo GGUF.
//!
//! Las gramáticas son cortas y cerradas, así que se reconocen a mano sobre
//! bytes ASCII en lugar de compilar expresiones regulares.

pub(super) const GGUF_EXTENSION: &str = ".gguf";
const SPLIT_DIGITS: usize = 5;
const MAX_QUANT_VARIANT_PARTS: usize = 3;
const MAX_QUANT_VARIANT_PART_LEN: usize = 3;

/// Devuelve la cuantización (en mayúsculas) de una ruta de HF, como
/// `BF16/model-BF16-00001-of-00002.gguf`. Se mira primero el nombre del
/// archivo y, si no la tiene, las carpetas de derecha a izquierda.
pub fn extract_quant(path: &str) -> Option<String> {
    let mut components = path.rsplit('/');
    let file_name = components.next()?;
    quant_in_name(strip_split_suffix(
        strip_gguf_extension(file_name).unwrap_or(file_name),
    ))
    .or_else(|| components.find_map(quant_in_name))
}

/// Quita la extensión `.gguf` (sin distinguir mayúsculas); `None` si no la tiene.
pub(super) fn strip_gguf_extension(file_name: &str) -> Option<&str> {
    let split_at = file_name.len().checked_sub(GGUF_EXTENSION.len())?;
    let (stem, ext) = file_name.split_at_checked(split_at)?;
    ext.eq_ignore_ascii_case(GGUF_EXTENSION).then_some(stem)
}

/// Sufijo `-NNNNN-of-NNNNN` de los archivos divididos.
pub(super) struct SplitSuffix {
    pub prefix_len: usize,
    pub index: u32,
    pub total: u32,
}

pub(super) fn parse_split_suffix(stem: &str) -> Option<SplitSuffix> {
    let bytes = stem.as_bytes();
    let suffix_len = 1 + SPLIT_DIGITS + "-of-".len() + SPLIT_DIGITS;
    let start = bytes.len().checked_sub(suffix_len)?;
    let suffix = &bytes[start..];
    let index_range = 1..1 + SPLIT_DIGITS;
    let total_range = suffix_len - SPLIT_DIGITS..suffix_len;
    let digits =
        |range: &std::ops::Range<usize>| suffix[range.clone()].iter().all(u8::is_ascii_digit);
    let is_split = suffix[0] == b'-'
        && digits(&index_range)
        && suffix[1 + SPLIT_DIGITS..].starts_with(b"-of-")
        && digits(&total_range);
    if !is_split {
        return None;
    }
    let number =
        |range: std::ops::Range<usize>| stem[start + range.start..start + range.end].parse().ok();
    Some(SplitSuffix {
        prefix_len: start,
        index: number(index_range)?,
        total: number(total_range)?,
    })
}

/// Quita el sufijo `-NNNNN-of-NNNNN` de los archivos divididos.
fn strip_split_suffix(stem: &str) -> &str {
    match parse_split_suffix(stem) {
        Some(split) => &stem[..split.prefix_len],
        None => stem,
    }
}

fn quant_in_name(name: &str) -> Option<String> {
    name.rsplit(['-', '.'])
        .find(|token| is_quant_token(token))
        .map(str::to_ascii_uppercase)
}

fn is_quant_token(token: &str) -> bool {
    let upper = token.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "TQ1_0" | "TQ2_0" | "BF16" | "F16" | "F32" | "MXFP4" | "MXFP4_MOE"
    ) || is_integer_quant(upper.strip_prefix('I').unwrap_or(&upper))
}

/// `Q[1-8](_[0-9A-Z]{1,3}){0,3}`
fn is_integer_quant(token: &str) -> bool {
    let Some(rest) = token.strip_prefix('Q') else {
        return false;
    };
    let mut chars = rest.chars();
    if !chars.next().is_some_and(|bits| ('1'..='8').contains(&bits)) {
        return false;
    }
    let variants = chars.as_str();
    if variants.is_empty() {
        return true;
    }
    let Some(variants) = variants.strip_prefix('_') else {
        return false;
    };
    let parts: Vec<&str> = variants.split('_').collect();
    parts.len() <= MAX_QUANT_VARIANT_PARTS
        && parts.iter().all(|part| {
            (1..=MAX_QUANT_VARIANT_PART_LEN).contains(&part.len())
                && part
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn some(value: &str) -> Option<String> {
        Some(value.to_owned())
    }

    #[test]
    fn quant_simple_q4_0() {
        assert_eq!(extract_quant("Qwen3.8-27B-Q4_0.gguf"), some("Q4_0"));
    }

    #[test]
    fn quant_ud_prefix() {
        assert_eq!(
            extract_quant("Qwen3.8-27B-UD-Q4_K_XL.gguf"),
            some("Q4_K_XL")
        );
    }

    #[test]
    fn quant_iq_xxs() {
        assert_eq!(
            extract_quant("Qwen3.8-27B-UD-IQ2_XXS.gguf"),
            some("IQ2_XXS")
        );
    }

    #[test]
    fn quant_iq4_nl() {
        assert_eq!(extract_quant("model-IQ4_NL.gguf"), some("IQ4_NL"));
    }

    #[test]
    fn quant_dot_separator() {
        assert_eq!(extract_quant("llama-2-7b.Q4_K_M.gguf"), some("Q4_K_M"));
    }

    #[test]
    fn quant_lowercase_normalized() {
        assert_eq!(extract_quant("model-q8_0.gguf"), some("Q8_0"));
    }

    #[test]
    fn quant_bf16_split() {
        assert_eq!(
            extract_quant("BF16/Qwen3.8-27B-BF16-00001-of-00002.gguf"),
            some("BF16")
        );
    }

    #[test]
    fn quant_f16() {
        assert_eq!(extract_quant("model-f16.gguf"), some("F16"));
    }

    #[test]
    fn quant_mxfp4() {
        assert_eq!(extract_quant("gpt-oss-20b-MXFP4.gguf"), some("MXFP4"));
    }

    #[test]
    fn quant_tq1_0() {
        assert_eq!(extract_quant("model-TQ1_0.gguf"), some("TQ1_0"));
    }

    #[test]
    fn quant_from_directory() {
        assert_eq!(
            extract_quant("Q4_K_M/model-00001-of-00003.gguf"),
            some("Q4_K_M")
        );
    }

    #[test]
    fn quant_none() {
        assert_eq!(extract_quant("model.gguf"), None);
        assert_eq!(extract_quant("dir/model-7b.gguf"), None);
    }

    #[test]
    fn quant_model_name_digits_not_matched() {
        assert_eq!(extract_quant("Qwen2.5-7B-Instruct.gguf"), None);
    }

    #[test]
    fn quant_empty_path() {
        assert_eq!(extract_quant(""), None);
    }

    #[test]
    fn quant_rejects_out_of_grammar_tokens() {
        assert_eq!(extract_quant("model-Q9_0.gguf"), None);
        assert_eq!(extract_quant("model-Q4_ABCD.gguf"), None);
        assert_eq!(extract_quant("model-Q4_A_B_C_D.gguf"), None);
        assert_eq!(extract_quant("model-TQ3_0.gguf"), None);
        assert_eq!(extract_quant("model-ITQ1_0.gguf"), None);
        assert_eq!(extract_quant("model-Q4_.gguf"), None);
    }

    #[test]
    fn quant_rightmost_wins() {
        assert_eq!(extract_quant("Q8_0-distill-Q4_K_M.gguf"), some("Q4_K_M"));
        assert_eq!(extract_quant("a-Q8_0-Q4_K_M.gguf"), some("Q4_K_M"));
    }

    #[test]
    fn quant_mxfp4_moe() {
        assert_eq!(extract_quant("m-MXFP4_MOE.gguf"), some("MXFP4_MOE"));
    }

    #[test]
    fn quant_file_name_wins_over_directory() {
        assert_eq!(extract_quant("Q8_0/model-Q4_K_M.gguf"), some("Q4_K_M"));
    }
}
