//! Clasificación de los assets de una release de llama.cpp a partir de los tokens de su nombre.
use crate::runtime::build_number;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Os {
    Windows,
    MacOs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Arch {
    X86_64,
    Aarch64,
}

// Las variantes van de menos a más capacidad: el orden derivado las compara.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CpuIsa {
    Generic,
    Avx,
    Avx2,
    Avx512,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Cpu { isa: CpuIsa },
    Cuda { major: u32, minor: u32 },
    Vulkan,
    Metal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveFormat {
    Zip,
    TarGz,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    Main,
    Cudart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetDescriptor {
    pub kind: AssetKind,
    pub os: Os,
    pub arch: Arch,
    pub backend: Backend,
    pub format: ArchiveFormat,
}

/// Clasifica un asset por los tokens de su nombre; `None` si no es un build que la app use.
/// Distingue mayúsculas: los nombres reales de llama.cpp van en minúsculas.
pub fn classify_asset(name: &str) -> Option<AssetDescriptor> {
    let (stem, format) = split_extension(name)?;
    let mut tokens = stem.split('-').peekable();

    let kind = if tokens.next_if_eq(&"cudart").is_some() {
        AssetKind::Cudart
    } else {
        AssetKind::Main
    };
    tokens.next_if_eq(&"llama")?;
    // El tag de build es opcional y no forma parte de la clasificación.
    tokens.next_if(|token| build_number(token).is_some());
    tokens.next_if_eq(&"bin")?;
    let os = parse_os(tokens.next()?)?;

    let rest: Vec<&str> = tokens.collect();
    let (arch_token, backend_tokens) = rest.split_last()?;
    let arch = parse_arch(arch_token)?;
    let backend = parse_backend(backend_tokens, os, arch)?;

    if kind == AssetKind::Cudart && !matches!(backend, Backend::Cuda { .. }) {
        return None;
    }
    Some(AssetDescriptor {
        kind,
        os,
        arch,
        backend,
        format,
    })
}

fn split_extension(name: &str) -> Option<(&str, ArchiveFormat)> {
    if let Some(stem) = name.strip_suffix(".zip") {
        Some((stem, ArchiveFormat::Zip))
    } else {
        name.strip_suffix(".tar.gz")
            .map(|stem| (stem, ArchiveFormat::TarGz))
    }
}

fn parse_os(token: &str) -> Option<Os> {
    match token {
        "win" => Some(Os::Windows),
        "macos" => Some(Os::MacOs),
        _ => None,
    }
}

fn parse_arch(token: &str) -> Option<Arch> {
    match token {
        "x64" => Some(Arch::X86_64),
        "arm64" => Some(Arch::Aarch64),
        _ => None,
    }
}

fn parse_backend(tokens: &[&str], os: Os, arch: Arch) -> Option<Backend> {
    match tokens {
        [] if os == Os::MacOs => Some(match arch {
            Arch::Aarch64 => Backend::Metal,
            Arch::X86_64 => Backend::Cpu {
                isa: CpuIsa::Generic,
            },
        }),
        ["cpu" | "noavx"] => Some(Backend::Cpu {
            isa: CpuIsa::Generic,
        }),
        ["avx"] => Some(Backend::Cpu { isa: CpuIsa::Avx }),
        ["avx2"] => Some(Backend::Cpu { isa: CpuIsa::Avx2 }),
        ["avx512"] => Some(Backend::Cpu {
            isa: CpuIsa::Avx512,
        }),
        ["vulkan"] => Some(Backend::Vulkan),
        ["cuda", version] => parse_cuda_version(version.strip_prefix("cu").unwrap_or(version)),
        [version] => parse_cuda_version(version.strip_prefix("cu")?),
        _ => None,
    }
}

/// `12.4`, sin prefijo; exige exactamente `major.minor` numéricos.
fn parse_cuda_version(token: &str) -> Option<Backend> {
    let (major, minor) = token.split_once('.')?;
    Some(Backend::Cuda {
        major: parse_digits(major)?,
        minor: parse_digits(minor)?,
    })
}

fn parse_digits(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GENERIC: Backend = Backend::Cpu {
        isa: CpuIsa::Generic,
    };

    fn descriptor(
        kind: AssetKind,
        os: Os,
        arch: Arch,
        backend: Backend,
        format: ArchiveFormat,
    ) -> Option<AssetDescriptor> {
        Some(AssetDescriptor {
            kind,
            os,
            arch,
            backend,
            format,
        })
    }

    fn main_win(arch: Arch, backend: Backend) -> Option<AssetDescriptor> {
        descriptor(
            AssetKind::Main,
            Os::Windows,
            arch,
            backend,
            ArchiveFormat::Zip,
        )
    }

    fn cudart_win(arch: Arch, major: u32, minor: u32) -> Option<AssetDescriptor> {
        descriptor(
            AssetKind::Cudart,
            Os::Windows,
            arch,
            Backend::Cuda { major, minor },
            ArchiveFormat::Zip,
        )
    }

    fn macos(arch: Arch, backend: Backend, format: ArchiveFormat) -> Option<AssetDescriptor> {
        descriptor(AssetKind::Main, Os::MacOs, arch, backend, format)
    }

    fn cuda(major: u32, minor: u32) -> Backend {
        Backend::Cuda { major, minor }
    }

    fn assert_table(table: &[(&str, Option<AssetDescriptor>)]) {
        for (name, expected) in table {
            assert_eq!(classify_asset(name), *expected, "{name}");
        }
    }

    fn assert_all_none(names: &[&str]) {
        for name in names {
            assert_eq!(classify_asset(name), None, "{name}");
        }
    }

    #[test]
    fn clasifica_assets_actuales() {
        use Arch::{Aarch64, X86_64};
        use ArchiveFormat::TarGz;
        assert_table(&[
            (
                "llama-b12780-bin-win-cpu-x64.zip",
                main_win(X86_64, GENERIC),
            ),
            (
                "llama-b12780-bin-win-cpu-arm64.zip",
                main_win(Aarch64, GENERIC),
            ),
            (
                "llama-b12780-bin-win-cuda-12.4-x64.zip",
                main_win(X86_64, cuda(12, 4)),
            ),
            (
                "llama-b12780-bin-win-cuda-13.5-x64.zip",
                main_win(X86_64, cuda(13, 5)),
            ),
            (
                "llama-b12780-bin-win-cuda-13.5-arm64.zip",
                main_win(Aarch64, cuda(13, 5)),
            ),
            (
                "cudart-llama-bin-win-cuda-12.4-x64.zip",
                cudart_win(X86_64, 12, 4),
            ),
            (
                "cudart-llama-bin-win-cuda-13.5-x64.zip",
                cudart_win(X86_64, 13, 5),
            ),
            (
                "cudart-llama-bin-win-cuda-13.5-arm64.zip",
                cudart_win(Aarch64, 13, 5),
            ),
            (
                "llama-b12780-bin-win-vulkan-x64.zip",
                main_win(X86_64, Backend::Vulkan),
            ),
            (
                "llama-b12780-bin-win-vulkan-arm64.zip",
                main_win(Aarch64, Backend::Vulkan),
            ),
            (
                "llama-b12780-bin-macos-arm64.tar.gz",
                macos(Aarch64, Backend::Metal, TarGz),
            ),
            (
                "llama-b12780-bin-macos-x64.tar.gz",
                macos(X86_64, GENERIC, TarGz),
            ),
        ]);
    }

    #[test]
    fn clasifica_cudart_sin_tag() {
        assert_eq!(
            classify_asset("cudart-llama-bin-win-cuda-13.5-x64.zip"),
            cudart_win(Arch::X86_64, 13, 5)
        );
    }

    #[test]
    fn clasifica_cudart_con_tag() {
        assert_eq!(
            classify_asset("cudart-llama-b12780-bin-win-cuda-12.4-x64.zip"),
            cudart_win(Arch::X86_64, 12, 4)
        );
    }

    #[test]
    fn clasifica_nombres_antiguos() {
        use Arch::{Aarch64, X86_64};
        let isa = |isa| Backend::Cpu { isa };
        assert_table(&[
            (
                "llama-b4500-bin-win-avx2-x64.zip",
                main_win(X86_64, isa(CpuIsa::Avx2)),
            ),
            (
                "llama-b4500-bin-win-avx512-x64.zip",
                main_win(X86_64, isa(CpuIsa::Avx512)),
            ),
            (
                "llama-b4500-bin-win-avx-x64.zip",
                main_win(X86_64, isa(CpuIsa::Avx)),
            ),
            (
                "llama-b4500-bin-win-noavx-x64.zip",
                main_win(X86_64, GENERIC),
            ),
            (
                "llama-b4500-bin-win-cuda-cu12.4-x64.zip",
                main_win(X86_64, cuda(12, 4)),
            ),
            (
                "cudart-llama-bin-win-cu12.4-x64.zip",
                cudart_win(X86_64, 12, 4),
            ),
            (
                "llama-b4500-bin-macos-arm64.zip",
                macos(Aarch64, Backend::Metal, ArchiveFormat::Zip),
            ),
        ]);
    }

    #[test]
    fn descarta_backends_fuera_de_alcance() {
        assert_all_none(&[
            "llama-b12780-bin-win-sycl-x64.zip",
            "llama-b12780-bin-win-rocm-10.0-x64.zip",
            "llama-b12780-bin-win-openvino-2026.4.1-x64.zip",
            "llama-b12780-bin-win-opencl-adreno-arm64.zip",
            "llama-b12780-bin-win-hip-radeon-x64.zip",
        ]);
    }

    #[test]
    fn descarta_otros_so_y_formatos() {
        assert_all_none(&[
            "llama-b12780-bin-ubuntu-x64.tar.gz",
            "llama-b12780-bin-android-arm64.tar.gz",
            "llama-b12780-xcframework.zip",
            "llama-b12780-ui.tar.gz",
            "nightly-tag.txt",
            "llama-b1-bin-win-cpu-x64.7z",
        ]);
    }

    #[test]
    fn descarta_nombres_malformados() {
        assert_all_none(&[
            "",
            "llama-",
            "llama-bin-win.zip",
            "llama-b1-bin-win-cuda-x64.zip",
            "llama-b1-bin-win-cuda-12-x64.zip",
            "llama-b1-bin-win-cucu12.4-x64.zip",
        ]);
    }

    #[test]
    fn clasificacion_sensible_a_mayusculas() {
        assert_all_none(&[
            "LLAMA-B1-BIN-WIN-CPU-X64.ZIP",
            "LLAMA-b1-BIN-win-cpu-x64.zip",
            "llama-b1-bin-WIN-cpu-x64.zip",
            "llama-b1-bin-win-CPU-x64.zip",
            "llama-b1-bin-win-cpu-X64.zip",
        ]);
    }
}
