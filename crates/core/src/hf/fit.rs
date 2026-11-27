//! Encaje orientativo de un modelo en la memoria del equipo y elección de la mejor variante.
use serde::Serialize;

use super::files::GgufVariant;
use crate::runtime::HardwareInfo;

pub const SAFETY_MARGIN_PERCENT: u64 = 20;
const USABLE_PERCENT: u8 = 100 - SAFETY_MARGIN_PERCENT as u8;
const BYTES_PER_MIB: u128 = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Fit {
    Gpu,
    Ram,
    NoFit,
}

/// Memoria utilizable (ya descontado el margen de seguridad), no la total.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MemoryBudget {
    pub gpu_bytes: Option<u64>,
    pub ram_bytes: Option<u64>,
    pub unified: bool,
    pub usable_percent: u8,
}

impl MemoryBudget {
    pub fn unknown() -> Self {
        Self {
            gpu_bytes: None,
            ram_bytes: None,
            unified: false,
            usable_percent: USABLE_PERCENT,
        }
    }

    pub fn from_hardware(hardware: &HardwareInfo) -> Self {
        let ram_bytes = hardware.ram_bytes.map(u128::from).map(usable);
        let gpu_bytes = if hardware.metal {
            ram_bytes
        } else {
            largest_vram_bytes(hardware).map(usable)
        };
        Self {
            gpu_bytes,
            ram_bytes,
            unified: hardware.metal,
            usable_percent: USABLE_PERCENT,
        }
    }
}

/// Con varias GPUs se usa la de más VRAM, no la suma.
fn largest_vram_bytes(hardware: &HardwareInfo) -> Option<u128> {
    let largest_mib = hardware.gpus.iter().filter_map(|gpu| gpu.vram_mib).max()?;
    Some(u128::from(largest_mib) * BYTES_PER_MIB)
}

fn usable(total_bytes: u128) -> u64 {
    let usable = total_bytes * u128::from(USABLE_PERCENT) / 100;
    u64::try_from(usable).unwrap_or(u64::MAX)
}

pub fn fit(required: u64, budget: &MemoryBudget) -> Option<Fit> {
    if budget.gpu_bytes.is_some_and(|gpu| required <= gpu) {
        return Some(Fit::Gpu);
    }
    let ram = budget.ram_bytes?;
    Some(if required <= ram {
        Fit::Ram
    } else {
        Fit::NoFit
    })
}

/// Entre las variantes completas, la mayor que cabe en la GPU; si no hay,
/// la mayor que cabe en la RAM. Los empates los gana el `name` menor.
pub fn recommended(candidates: &[(&GgufVariant, Option<Fit>)]) -> Option<usize> {
    best_with_fit(candidates, Fit::Gpu).or_else(|| best_with_fit(candidates, Fit::Ram))
}

fn best_with_fit(candidates: &[(&GgufVariant, Option<Fit>)], wanted: Fit) -> Option<usize> {
    candidates
        .iter()
        .enumerate()
        .filter(|(_, (variant, fit))| variant.complete && *fit == Some(wanted))
        .max_by(|(_, (a, _)), (_, (b, _))| {
            a.total_bytes
                .cmp(&b.total_bytes)
                .then_with(|| b.name.cmp(&a.name))
        })
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Arch, GpuInfo, Os};

    const GIB_32: u64 = 34_359_738_368;
    const GIB_16: u64 = 17_179_869_184;

    fn hardware(
        gpus: Vec<Option<u64>>,
        metal: bool,
        vulkan: bool,
        ram: Option<u64>,
    ) -> HardwareInfo {
        HardwareInfo {
            os: Os::Windows,
            arch: Arch::X86_64,
            avx2: true,
            avx512: false,
            gpus: gpus
                .into_iter()
                .map(|vram_mib| GpuInfo {
                    name: "GPU".to_owned(),
                    vram_mib,
                })
                .collect(),
            driver_cuda: None,
            vulkan_available: vulkan,
            metal,
            ram_bytes: ram,
        }
    }

    fn rtx_4070_32gib() -> MemoryBudget {
        MemoryBudget::from_hardware(&hardware(vec![Some(12282)], false, false, Some(GIB_32)))
    }

    fn budget(gpu: Option<u64>, ram: Option<u64>, unified: bool) -> MemoryBudget {
        MemoryBudget {
            gpu_bytes: gpu,
            ram_bytes: ram,
            unified,
            usable_percent: 80,
        }
    }

    #[test]
    fn budget_single_nvidia_gpu() {
        let b = rtx_4070_32gib();
        assert_eq!(b.gpu_bytes, Some(10_302_888_345));
        assert_eq!(b.ram_bytes, Some(27_487_790_694));
        assert!(!b.unified);
        assert_eq!(b.usable_percent, 80);
    }

    #[test]
    fn budget_uses_largest_gpu() {
        let b = MemoryBudget::from_hardware(&hardware(
            vec![Some(12282), Some(15360)],
            false,
            false,
            Some(GIB_32),
        ));
        assert_eq!(b.gpu_bytes, Some(12_884_901_888));
    }

    #[test]
    fn budget_gpu_without_vram_figure_ignored() {
        let b = MemoryBudget::from_hardware(&hardware(vec![None], false, false, Some(GIB_32)));
        assert_eq!(b.gpu_bytes, None);
        assert_eq!(b.ram_bytes, Some(27_487_790_694));
    }

    #[test]
    fn budget_metal_unified() {
        let b = MemoryBudget::from_hardware(&hardware(vec![], true, false, Some(GIB_16)));
        assert_eq!(b.gpu_bytes, Some(13_743_895_347));
        assert_eq!(b.gpu_bytes, b.ram_bytes);
        assert!(b.unified);
    }

    #[test]
    fn budget_metal_without_ram() {
        let b = MemoryBudget::from_hardware(&hardware(vec![], true, false, None));
        assert_eq!(b.gpu_bytes, None);
        assert_eq!(b.ram_bytes, None);
        assert!(b.unified);
    }

    #[test]
    fn budget_vulkan_only_ram() {
        let b = MemoryBudget::from_hardware(&hardware(vec![], false, true, Some(GIB_16)));
        assert_eq!(b.gpu_bytes, None);
        assert_eq!(b.ram_bytes, Some(13_743_895_347));
    }

    #[test]
    fn budget_ram_unknown() {
        let b = MemoryBudget::from_hardware(&hardware(vec![Some(12282)], false, false, None));
        assert_eq!(b.gpu_bytes, Some(10_302_888_345));
        assert_eq!(b.ram_bytes, None);
    }

    #[test]
    fn budget_saturates() {
        let b = MemoryBudget::from_hardware(&hardware(
            vec![Some(u64::MAX)],
            false,
            false,
            Some(u64::MAX),
        ));
        assert_eq!(b.gpu_bytes, Some(u64::MAX));
        assert_eq!(b.ram_bytes, Some(14_757_395_258_967_641_292));
    }

    #[test]
    fn budget_unknown() {
        let b = MemoryBudget::unknown();
        assert_eq!(b.gpu_bytes, None);
        assert_eq!(b.ram_bytes, None);
        assert!(!b.unified);
        assert_eq!(b.usable_percent, 80);
    }

    #[test]
    fn fit_gpu_when_fits_vram() {
        assert_eq!(fit(6_530_612_736, &rtx_4070_32gib()), Some(Fit::Gpu));
    }

    #[test]
    fn fit_boundary_equal_is_gpu() {
        assert_eq!(fit(10_302_888_345, &rtx_4070_32gib()), Some(Fit::Gpu));
    }

    #[test]
    fn fit_one_byte_over_vram_is_ram() {
        assert_eq!(fit(10_302_888_346, &rtx_4070_32gib()), Some(Fit::Ram));
    }

    #[test]
    fn fit_ram_boundary() {
        assert_eq!(fit(27_487_790_694, &rtx_4070_32gib()), Some(Fit::Ram));
        assert_eq!(fit(27_487_790_695, &rtx_4070_32gib()), Some(Fit::NoFit));
    }

    #[test]
    fn fit_only_ram_known() {
        let b = budget(None, Some(13_743_895_347), false);
        assert_eq!(fit(6_530_612_736, &b), Some(Fit::Ram));
    }

    #[test]
    fn fit_unified_never_ram() {
        let b = MemoryBudget::from_hardware(&hardware(vec![], true, false, Some(GIB_16)));
        assert_eq!(fit(6_530_612_736, &b), Some(Fit::Gpu));
        assert_eq!(fit(15_000_000_000, &b), Some(Fit::NoFit));
    }

    #[test]
    fn fit_ram_unknown_over_gpu_is_none() {
        let b = budget(Some(10_302_888_345), None, false);
        assert_eq!(fit(15_000_000_000, &b), None);
    }

    #[test]
    fn fit_ram_unknown_within_gpu() {
        let b = budget(Some(10_302_888_345), None, false);
        assert_eq!(fit(6_530_612_736, &b), Some(Fit::Gpu));
    }

    #[test]
    fn fit_unknown_budget() {
        assert_eq!(fit(1, &MemoryBudget::unknown()), None);
        assert_eq!(fit(u64::MAX, &MemoryBudget::unknown()), None);
    }

    fn variant(name: &str, total_bytes: u64, complete: bool) -> GgufVariant {
        GgufVariant {
            name: name.to_owned(),
            quant: None,
            parts: Vec::new(),
            total_bytes,
            complete,
        }
    }

    fn recommend(items: &[(GgufVariant, Option<Fit>)]) -> Option<usize> {
        let candidates: Vec<(&GgufVariant, Option<Fit>)> =
            items.iter().map(|(v, f)| (v, *f)).collect();
        recommended(&candidates)
    }

    #[test]
    fn recommend_largest_gpu_fit() {
        let items = [
            (variant("a", 4_000_000_000, true), Some(Fit::Gpu)),
            (variant("b", 8_000_000_000, true), Some(Fit::Gpu)),
            (variant("c", 16_000_000_000, true), Some(Fit::Ram)),
        ];
        assert_eq!(recommend(&items), Some(1));
    }

    #[test]
    fn recommend_gpu_beats_larger_ram() {
        let items = [
            (variant("a", 4_000_000_000, true), Some(Fit::Gpu)),
            (variant("b", 16_000_000_000, true), Some(Fit::Ram)),
        ];
        assert_eq!(recommend(&items), Some(0));
    }

    #[test]
    fn recommend_falls_back_to_ram() {
        let items = [
            (variant("a", 4_000_000_000, true), Some(Fit::Ram)),
            (variant("b", 8_000_000_000, true), Some(Fit::Ram)),
            (variant("c", 16_000_000_000, true), Some(Fit::NoFit)),
        ];
        assert_eq!(recommend(&items), Some(1));
    }

    #[test]
    fn recommend_none_when_nothing_fits() {
        let items = [
            (variant("a", 4_000_000_000, true), Some(Fit::NoFit)),
            (variant("b", 8_000_000_000, true), Some(Fit::NoFit)),
        ];
        assert_eq!(recommend(&items), None);
    }

    #[test]
    fn recommend_skips_incomplete() {
        let items = [
            (variant("a", 4_000_000_000, true), Some(Fit::Gpu)),
            (variant("b", 8_000_000_000, false), Some(Fit::Gpu)),
        ];
        assert_eq!(recommend(&items), Some(0));
    }

    #[test]
    fn recommend_tie_smallest_name() {
        let items = [
            (variant("b", 8_000_000_000, true), Some(Fit::Gpu)),
            (variant("a", 8_000_000_000, true), Some(Fit::Gpu)),
        ];
        assert_eq!(recommend(&items), Some(1));
    }

    #[test]
    fn recommend_ignores_unknown_fit() {
        let items = [
            (variant("a", 4_000_000_000, true), None),
            (variant("b", 8_000_000_000, true), None),
        ];
        assert_eq!(recommend(&items), None);
    }

    #[test]
    fn recommend_empty() {
        assert_eq!(recommend(&[]), None);
    }
}
