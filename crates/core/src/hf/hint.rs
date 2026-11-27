//! Insignia orientativa de la búsqueda: encaje estimado con una cuantización típica.
use serde::Serialize;

use super::fit::{Fit, MemoryBudget, fit};
use super::memory::{MemoryEstimate, estimate_memory, estimation_ctx};
use super::model::GgufInfo;

/// Q4_K_M ocupa ≈ 4,85 bits por parámetro.
pub const TYPICAL_QUANT_MILLIBYTES_PER_PARAM: u64 = 606;
const MILLI: u128 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FitHint {
    pub estimate: MemoryEstimate,
    pub fit: Option<Fit>,
}

pub fn typical_quant_bytes(params_total: u64) -> u64 {
    let bytes = u128::from(params_total) * u128::from(TYPICAL_QUANT_MILLIBYTES_PER_PARAM) / MILLI;
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

/// `None` si el listado no trae el número de parámetros (o vale 0).
pub fn fit_hint(gguf: Option<&GgufInfo>, budget: &MemoryBudget) -> Option<FitHint> {
    let gguf = gguf?;
    let total = gguf.total.filter(|&total| total > 0)?;
    let estimate = estimate_memory(
        typical_quant_bytes(total),
        Some(total),
        estimation_ctx(gguf.context_length),
    );
    Some(FitHint {
        estimate,
        fit: fit(estimate.bytes, budget),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Arch, GpuInfo, HardwareInfo, Os};

    const GIB_32: u64 = 34_359_738_368;

    fn gguf(total: Option<u64>, context_length: Option<u64>) -> GgufInfo {
        GgufInfo {
            total,
            architecture: None,
            context_length,
        }
    }

    fn rtx_4070_32gib() -> MemoryBudget {
        MemoryBudget::from_hardware(&HardwareInfo {
            os: Os::Windows,
            arch: Arch::X86_64,
            avx2: true,
            avx512: false,
            gpus: vec![GpuInfo {
                name: "GPU".to_owned(),
                vram_mib: Some(12282),
            }],
            driver_cuda: None,
            vulkan_available: false,
            metal: false,
            ram_bytes: Some(GIB_32),
        })
    }

    fn llama31_8b() -> GgufInfo {
        gguf(Some(8_030_261_312), Some(131_072))
    }

    fn llama33_70b() -> GgufInfo {
        gguf(Some(70_553_706_560), Some(131_072))
    }

    #[test]
    fn typical_quant_bytes_llama2_7b() {
        assert_eq!(typical_quant_bytes(6_738_415_616), 4_083_479_863);
    }

    #[test]
    fn typical_quant_bytes_8b() {
        assert_eq!(typical_quant_bytes(8_030_261_312), 4_866_338_355);
    }

    #[test]
    fn typical_quant_bytes_zero() {
        assert_eq!(typical_quant_bytes(0), 0);
    }

    #[test]
    fn typical_quant_bytes_max_no_panic() {
        assert_eq!(typical_quant_bytes(u64::MAX), 11_178_726_908_667_988_278);
    }

    #[test]
    fn fit_hint_llama31_8b_gpu() {
        let hint = fit_hint(Some(&llama31_8b()), &rtx_4070_32gib()).unwrap();
        assert_eq!(
            hint.estimate,
            MemoryEstimate {
                bytes: 6_481_006_131,
                kv_included: true
            }
        );
        assert_eq!(hint.fit, Some(Fit::Gpu));
    }

    #[test]
    fn fit_hint_llama33_70b_no_fit_32gib() {
        let hint = fit_hint(Some(&llama33_70b()), &rtx_4070_32gib()).unwrap();
        assert_eq!(hint.estimate.bytes, 52_761_967_679);
        assert_eq!(hint.fit, Some(Fit::NoFit));
    }

    #[test]
    fn fit_hint_llama33_70b_ram_64gib() {
        let budget = MemoryBudget {
            gpu_bytes: Some(10_302_888_345),
            ram_bytes: Some(54_975_581_388),
            unified: false,
            usable_percent: 80,
        };
        let hint = fit_hint(Some(&llama33_70b()), &budget).unwrap();
        assert_eq!(hint.fit, Some(Fit::Ram));
    }

    #[test]
    fn fit_hint_missing_context_uses_fallback() {
        let info = gguf(Some(8_000_000_000), None);
        let hint = fit_hint(Some(&info), &rtx_4070_32gib()).unwrap();
        assert_eq!(hint.estimate.bytes, 5_921_741_824);
    }

    #[test]
    fn fit_hint_unknown_budget() {
        let hint = fit_hint(Some(&llama31_8b()), &MemoryBudget::unknown()).unwrap();
        assert_eq!(hint.estimate.bytes, 6_481_006_131);
        assert_eq!(hint.fit, None);
    }

    #[test]
    fn fit_hint_without_gguf() {
        assert_eq!(fit_hint(None, &rtx_4070_32gib()), None);
    }

    #[test]
    fn fit_hint_without_total() {
        let info = gguf(None, Some(4096));
        assert_eq!(fit_hint(Some(&info), &rtx_4070_32gib()), None);
    }

    #[test]
    fn fit_hint_total_zero() {
        let info = gguf(Some(0), Some(4096));
        assert_eq!(fit_hint(Some(&info), &rtx_4070_32gib()), None);
    }
}
