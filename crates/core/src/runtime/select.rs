//! Elección del asset de llama.cpp (y su cudart) que mejor encaja con el hardware.
use serde::Deserialize;

use crate::runtime::{
    Asset, AssetDescriptor, AssetKind, Backend, CpuIsa, HardwareInfo, Release, RuntimeError,
    classify_asset,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendPreference {
    Auto,
    Cpu,
    Cuda,
    Vulkan,
    Metal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPlan {
    pub tag: String,
    pub backend: Backend,
    pub main: Asset,
    pub cudart: Option<Asset>,
}

/// Una opción de instalación posible en esta máquina.
struct Candidate<'a> {
    backend: Backend,
    main: &'a Asset,
    cudart: Option<&'a Asset>,
}

type ClassifiedAsset<'a> = (&'a Asset, AssetDescriptor);

/// Backends instalables con esta release, en el orden CUDA > Metal > Vulkan > CPU.
pub fn available_backends(release: &Release, hw: &HardwareInfo) -> Vec<Backend> {
    candidates(release, hw)
        .into_iter()
        .map(|candidate| candidate.backend)
        .collect()
}

pub fn select_install(
    release: &Release,
    hw: &HardwareInfo,
    preference: BackendPreference,
) -> Result<InstallPlan, RuntimeError> {
    let candidates = candidates(release, hw);
    if candidates.is_empty() {
        return Err(RuntimeError::NoCompatibleAsset);
    }
    let chosen = candidates
        .into_iter()
        .find(|candidate| satisfies(preference, candidate.backend))
        .ok_or(RuntimeError::BackendUnavailable)?;
    Ok(InstallPlan {
        tag: release.tag_name.clone(),
        backend: chosen.backend,
        main: chosen.main.clone(),
        cudart: chosen.cudart.cloned(),
    })
}

/// Predicado para `latest_build_release`: la release tiene algún asset utilizable.
pub fn is_compatible(release: &Release, hw: &HardwareInfo) -> bool {
    !candidates(release, hw).is_empty()
}

fn satisfies(preference: BackendPreference, backend: Backend) -> bool {
    matches!(
        (preference, backend),
        (BackendPreference::Auto, _)
            | (BackendPreference::Cpu, Backend::Cpu { .. })
            | (BackendPreference::Cuda, Backend::Cuda { .. })
            | (BackendPreference::Vulkan, Backend::Vulkan)
            | (BackendPreference::Metal, Backend::Metal)
    )
}

fn candidates<'a>(release: &'a Release, hw: &HardwareInfo) -> Vec<Candidate<'a>> {
    let assets: Vec<ClassifiedAsset> = release
        .assets
        .iter()
        .filter_map(|asset| Some((asset, classify_asset(&asset.name)?)))
        .filter(|(_, descriptor)| descriptor.os == hw.os && descriptor.arch == hw.arch)
        .collect();
    [
        cuda_candidate(&assets, hw),
        simple_candidate(&assets, hw.metal, Backend::Metal),
        simple_candidate(&assets, hw.vulkan_available, Backend::Vulkan),
        cpu_candidate(&assets, hw),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn assets_of<'a, 's>(
    assets: &'s [ClassifiedAsset<'a>],
    kind: AssetKind,
) -> impl Iterator<Item = (&'a Asset, Backend)> + 's {
    assets
        .iter()
        .filter(move |(_, descriptor)| descriptor.kind == kind)
        .map(|(asset, descriptor)| (*asset, descriptor.backend))
}

/// Metal y Vulkan: un único asset por plataforma, disponible solo si el hardware lo permite.
fn simple_candidate<'a>(
    assets: &[ClassifiedAsset<'a>],
    hardware_supports: bool,
    backend: Backend,
) -> Option<Candidate<'a>> {
    if !hardware_supports {
        return None;
    }
    let (main, _) = assets_of(assets, AssetKind::Main).find(|(_, found)| *found == backend)?;
    Some(Candidate {
        backend,
        main,
        cudart: None,
    })
}

/// La mayor versión de CUDA que el driver admite (misma versión mayor o inferior) y que
/// tiene su cudart; sin cudart el build no arranca en máquinas sin el toolkit.
fn cuda_candidate<'a>(assets: &[ClassifiedAsset<'a>], hw: &HardwareInfo) -> Option<Candidate<'a>> {
    let driver = hw.driver_cuda?;
    assets_of(assets, AssetKind::Main)
        .filter_map(|(main, backend)| {
            let Backend::Cuda { major, minor } = backend else {
                return None;
            };
            if major > driver.major {
                return None;
            }
            let (cudart, _) =
                assets_of(assets, AssetKind::Cudart).find(|(_, found)| *found == backend)?;
            let candidate = Candidate {
                backend,
                main,
                cudart: Some(cudart),
            };
            Some(((major, minor), candidate))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, candidate)| candidate)
}

/// El build con más instrucciones que la CPU soporta. `Generic` (la build actual) lleva
/// todas las variantes dentro y es el mínimo en las releases antiguas con un build por ISA.
fn cpu_candidate<'a>(assets: &[ClassifiedAsset<'a>], hw: &HardwareInfo) -> Option<Candidate<'a>> {
    assets_of(assets, AssetKind::Main)
        .filter_map(|(main, backend)| match backend {
            Backend::Cpu { isa } if cpu_supports(hw, isa) => Some((isa, main)),
            _ => None,
        })
        .max_by_key(|(isa, _)| *isa)
        .map(|(isa, main)| Candidate {
            backend: Backend::Cpu { isa },
            main,
            cudart: None,
        })
}

fn cpu_supports(hw: &HardwareInfo, isa: CpuIsa) -> bool {
    match isa {
        CpuIsa::Generic => true,
        CpuIsa::Avx | CpuIsa::Avx2 => hw.avx2 || hw.avx512,
        CpuIsa::Avx512 => hw.avx512,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Arch, CpuIsa, CudaVersion, Os, parse_releases};

    const CURRENT: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/releases_b12780.json"
    ));
    const LEGACY: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/releases_legacy.json"
    ));

    fn release_from(json: &str, tag: &str) -> Release {
        parse_releases(json)
            .unwrap()
            .into_iter()
            .find(|release| release.tag_name == tag)
            .unwrap()
    }

    fn b12780() -> Release {
        release_from(CURRENT, "b12780")
    }

    fn legacy() -> Release {
        release_from(LEGACY, "b4500")
    }

    fn hardware(os: Os, arch: Arch) -> HardwareInfo {
        HardwareInfo {
            os,
            arch,
            avx2: false,
            avx512: false,
            gpus: Vec::new(),
            driver_cuda: None,
            vulkan_available: false,
            metal: os == Os::MacOs && arch == Arch::Aarch64,
            ram_bytes: None,
        }
    }

    fn windows() -> HardwareInfo {
        hardware(Os::Windows, Arch::X86_64)
    }

    fn with_cuda(hw: HardwareInfo, major: u32, minor: u32) -> HardwareInfo {
        HardwareInfo {
            driver_cuda: Some(CudaVersion { major, minor }),
            ..hw
        }
    }

    fn with_vulkan(hw: HardwareInfo) -> HardwareInfo {
        HardwareInfo {
            vulkan_available: true,
            ..hw
        }
    }

    fn auto(release: &Release, hw: &HardwareInfo) -> InstallPlan {
        select_install(release, hw, BackendPreference::Auto).unwrap()
    }

    fn cudart_name(plan: &InstallPlan) -> Option<&str> {
        plan.cudart.as_ref().map(|asset| asset.name.as_str())
    }

    #[test]
    fn auto_cuda_13() {
        let plan = auto(&b12780(), &with_cuda(windows(), 13, 0));
        assert_eq!(plan.tag, "b12780");
        assert_eq!(
            plan.backend,
            Backend::Cuda {
                major: 13,
                minor: 5
            }
        );
        assert_eq!(plan.main.name, "llama-b12780-bin-win-cuda-13.5-x64.zip");
        assert_eq!(
            cudart_name(&plan),
            Some("cudart-llama-bin-win-cuda-13.5-x64.zip")
        );
    }

    #[test]
    fn auto_cuda_12() {
        let plan = auto(&b12780(), &with_cuda(windows(), 12, 8));
        assert_eq!(plan.main.name, "llama-b12780-bin-win-cuda-12.4-x64.zip");
        assert_eq!(
            cudart_name(&plan),
            Some("cudart-llama-bin-win-cuda-12.4-x64.zip")
        );
    }

    #[test]
    fn auto_cuda_driver_antiguo_vulkan() {
        let hw = with_vulkan(with_cuda(windows(), 11, 8));
        let plan = auto(&b12780(), &hw);
        assert_eq!(plan.backend, Backend::Vulkan);
        assert_eq!(plan.main.name, "llama-b12780-bin-win-vulkan-x64.zip");
        assert_eq!(plan.cudart, None);
    }

    #[test]
    fn auto_cpu() {
        let plan = auto(&b12780(), &windows());
        assert_eq!(
            plan.backend,
            Backend::Cpu {
                isa: CpuIsa::Generic
            }
        );
        assert_eq!(plan.main.name, "llama-b12780-bin-win-cpu-x64.zip");
    }

    #[test]
    fn auto_win_arm64() {
        let plan = auto(&b12780(), &hardware(Os::Windows, Arch::Aarch64));
        assert_eq!(plan.main.name, "llama-b12780-bin-win-cpu-arm64.zip");
    }

    #[test]
    fn auto_macos_arm64_metal() {
        let plan = auto(&b12780(), &hardware(Os::MacOs, Arch::Aarch64));
        assert_eq!(plan.backend, Backend::Metal);
        assert_eq!(plan.main.name, "llama-b12780-bin-macos-arm64.tar.gz");
    }

    #[test]
    fn auto_macos_x64() {
        let plan = auto(&b12780(), &hardware(Os::MacOs, Arch::X86_64));
        assert_eq!(
            plan.backend,
            Backend::Cpu {
                isa: CpuIsa::Generic
            }
        );
        assert_eq!(plan.main.name, "llama-b12780-bin-macos-x64.tar.gz");
    }

    #[test]
    fn cuda_sin_cudart_se_descarta() {
        let mut release = b12780();
        release
            .assets
            .retain(|asset| !asset.name.starts_with("cudart-"));
        let hw = with_vulkan(with_cuda(windows(), 13, 0));
        let plan = auto(&release, &hw);
        assert_eq!(plan.backend, Backend::Vulkan);
    }

    #[test]
    fn preferencia_explicita_cpu() {
        let hw = with_cuda(windows(), 13, 0);
        let plan = select_install(&b12780(), &hw, BackendPreference::Cpu).unwrap();
        assert_eq!(plan.main.name, "llama-b12780-bin-win-cpu-x64.zip");
    }

    #[test]
    fn preferencia_no_disponible() {
        let result = select_install(&b12780(), &windows(), BackendPreference::Cuda);
        assert!(matches!(result, Err(RuntimeError::BackendUnavailable)));
    }

    fn legacy_cpu_asset(avx2: bool, avx512: bool) -> String {
        let hw = HardwareInfo {
            avx2,
            avx512,
            ..windows()
        };
        auto(&legacy(), &hw).main.name
    }

    #[test]
    fn legacy_cpu_mejor_isa() {
        assert_eq!(
            legacy_cpu_asset(true, false),
            "llama-b4500-bin-win-avx2-x64.zip"
        );
    }

    #[test]
    fn legacy_cpu_avx512() {
        assert_eq!(
            legacy_cpu_asset(true, true),
            "llama-b4500-bin-win-avx512-x64.zip"
        );
    }

    #[test]
    fn legacy_cpu_sin_avx() {
        assert_eq!(
            legacy_cpu_asset(false, false),
            "llama-b4500-bin-win-noavx-x64.zip"
        );
    }

    #[test]
    fn legacy_cuda_cu() {
        let plan = auto(&legacy(), &with_cuda(windows(), 12, 2));
        assert_eq!(plan.main.name, "llama-b4500-bin-win-cuda-cu12.4-x64.zip");
        assert_eq!(
            cudart_name(&plan),
            Some("cudart-llama-bin-win-cu12.4-x64.zip")
        );
    }

    #[test]
    fn backends_disponibles_orden() {
        let hw = with_vulkan(with_cuda(windows(), 13, 0));
        assert_eq!(
            available_backends(&b12780(), &hw),
            [
                Backend::Cuda {
                    major: 13,
                    minor: 5
                },
                Backend::Vulkan,
                Backend::Cpu {
                    isa: CpuIsa::Generic
                },
            ]
        );
    }

    #[test]
    fn release_sin_compatibles() {
        let mut release = b12780();
        release.assets.retain(|asset| asset.name.contains("ubuntu"));
        assert!(matches!(
            select_install(&release, &windows(), BackendPreference::Auto),
            Err(RuntimeError::NoCompatibleAsset)
        ));
        assert!(!is_compatible(&release, &windows()));
        assert!(is_compatible(&b12780(), &windows()));
    }
}
