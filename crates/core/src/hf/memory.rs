//! Estimación orientativa de la memoria necesaria para ejecutar un modelo.
use serde::Serialize;

const FALLBACK_CTX: u32 = 4096;
const MAX_ESTIMATION_CTX: u32 = 8192;
const OVERHEAD_BYTES: u128 = 536_870_912;
const KV_BYTES_PER_TOKEN_PER_BILLION: u128 = 16_384;
const BILLION: u128 = 1_000_000_000;

/// Memoria estimada; `kv_included` es `false` cuando no se conocen los parámetros
/// y por tanto no se pudo sumar la caché KV.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MemoryEstimate {
    pub bytes: u64,
    pub kv_included: bool,
}

/// Contexto con el que se estima: el del modelo acotado a `MAX_ESTIMATION_CTX`,
/// o `FALLBACK_CTX` si no consta.
pub fn estimation_ctx(context_length: Option<u64>) -> u32 {
    match context_length {
        Some(n) if n > 0 => {
            u32::try_from(n).map_or(MAX_ESTIMATION_CTX, |n| n.min(MAX_ESTIMATION_CTX))
        }
        _ => FALLBACK_CTX,
    }
}

/// Archivo + caché KV (si se conocen los parámetros) + sobrecarga fija.
pub fn estimate_memory(file_bytes: u64, params_total: Option<u64>, ctx: u32) -> MemoryEstimate {
    let kv_bytes = params_total.map(|params| kv_cache_bytes(params, ctx));
    let total = u128::from(file_bytes) + OVERHEAD_BYTES + kv_bytes.unwrap_or(0);
    MemoryEstimate {
        bytes: u64::try_from(total).unwrap_or(u64::MAX),
        kv_included: kv_bytes.is_some(),
    }
}

/// Los bytes por token se truncan antes de multiplicar por el contexto.
fn kv_cache_bytes(params_total: u64, ctx: u32) -> u128 {
    let per_token = u128::from(params_total) * KV_BYTES_PER_TOKEN_PER_BILLION / BILLION;
    per_token * u128::from(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimation_ctx_caps_at_8192() {
        assert_eq!(estimation_ctx(Some(40_960)), 8192);
    }

    #[test]
    fn estimation_ctx_exact_8192() {
        assert_eq!(estimation_ctx(Some(8192)), 8192);
    }

    #[test]
    fn estimation_ctx_small_model() {
        assert_eq!(estimation_ctx(Some(2048)), 2048);
    }

    #[test]
    fn estimation_ctx_missing_falls_back() {
        assert_eq!(estimation_ctx(None), 4096);
    }

    #[test]
    fn estimation_ctx_zero_falls_back() {
        assert_eq!(estimation_ctx(Some(0)), 4096);
    }

    #[test]
    fn estimation_ctx_huge_value() {
        assert_eq!(estimation_ctx(Some(u64::MAX)), 8192);
    }

    #[test]
    fn estimate_with_params() {
        let e = estimate_memory(4_920_000_000, Some(8_000_000_000), 4096);
        assert_eq!(e.bytes, 5_993_741_824);
        assert!(e.kv_included);
    }

    #[test]
    fn estimate_scales_with_ctx_8192_example() {
        let e = estimate_memory(4_920_000_000, Some(8_000_000_000), 8192);
        assert_eq!(e.bytes, 6_530_612_736);
        assert!(e.kv_included);
    }

    #[test]
    fn estimate_without_params() {
        let e = estimate_memory(4_920_000_000, None, 4096);
        assert_eq!(e.bytes, 5_456_870_912);
        assert!(!e.kv_included);
    }

    #[test]
    fn estimate_ctx_zero() {
        let e = estimate_memory(1000, Some(8_000_000_000), 0);
        assert_eq!(e.bytes, 1000 + 536_870_912);
        assert!(e.kv_included);
    }

    #[test]
    fn estimate_kv_truncates() {
        let e = estimate_memory(0, Some(1), 1);
        assert_eq!(e.bytes, 536_870_912);
    }

    #[test]
    fn estimate_scales_with_ctx() {
        let e = estimate_memory(0, Some(8_000_000_000), 8192);
        assert_eq!(e.bytes, 536_870_912 + 1_073_741_824);
    }

    #[test]
    fn estimate_saturates() {
        let e = estimate_memory(u64::MAX, Some(u64::MAX), u32::MAX);
        assert_eq!(e.bytes, u64::MAX);
    }

    #[test]
    fn estimate_kv_truncates_per_token() {
        let e = estimate_memory(0, Some(8_030_261_312), 8192);
        assert_eq!(e.bytes, 1_077_796_864 + 536_870_912);
    }
}
