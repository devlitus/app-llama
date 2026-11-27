//! Detección del hardware relevante para elegir el build de llama.cpp.
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::runtime::process::hide_console;
use crate::runtime::{Arch, Os};

const NVIDIA_SMI: &str = "nvidia-smi";
const NVIDIA_SMI_GPU_QUERY: [&str; 2] = [
    "--query-gpu=name,memory.total",
    "--format=csv,noheader,nounits",
];
const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(25);
const CUDA_VERSION_LABEL: &str = "CUDA Version:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GpuInfo {
    pub name: String,
    pub vram_mib: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CudaVersion {
    pub major: u32,
    pub minor: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HardwareInfo {
    pub os: Os,
    pub arch: Arch,
    pub avx2: bool,
    pub avx512: bool,
    pub gpus: Vec<GpuInfo>,
    pub driver_cuda: Option<CudaVersion>,
    pub vulkan_available: bool,
    pub metal: bool,
}

/// Acceso al sistema detrás de un trait para poder simularlo en los tests.
pub trait SystemProbe: Send + Sync {
    fn os(&self) -> Os;
    fn arch(&self) -> Arch;
    fn cpu_has(&self, feature: &str) -> bool;
    /// Devuelve la salida estándar si el programa termina con código 0.
    fn run(&self, program: &str, args: &[&str]) -> Option<String>;
    fn file_exists(&self, path: &Path) -> bool;
    fn system_root(&self) -> Option<PathBuf>;
}

/// Salida de `nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits`.
pub fn parse_nvidia_smi_gpus(csv: &str) -> Vec<GpuInfo> {
    csv.lines().filter_map(parse_gpu_line).collect()
}

fn parse_gpu_line(line: &str) -> Option<GpuInfo> {
    let (name, vram) = line.trim().rsplit_once(',')?;
    Some(GpuInfo {
        name: name.trim().to_string(),
        vram_mib: vram.trim().parse().ok(),
    })
}

/// Busca `CUDA Version: X.Y` en la cabecera de `nvidia-smi` sin argumentos.
pub fn parse_nvidia_smi_cuda(header: &str) -> Option<CudaVersion> {
    let (_, after_label) = header.split_once(CUDA_VERSION_LABEL)?;
    let (major, minor) = after_label.split_whitespace().next()?.split_once('.')?;
    Some(CudaVersion {
        major: major.parse().ok()?,
        minor: minor.parse().ok()?,
    })
}

pub fn detect(probe: &dyn SystemProbe) -> HardwareInfo {
    let (os, arch) = (probe.os(), probe.arch());
    let is_x86_64 = arch == Arch::X86_64;
    let (gpus, driver_cuda) = if os == Os::Windows {
        detect_nvidia(probe)
    } else {
        (Vec::new(), None)
    };
    HardwareInfo {
        os,
        arch,
        avx2: is_x86_64 && probe.cpu_has("avx2"),
        avx512: is_x86_64 && probe.cpu_has("avx512f"),
        gpus,
        driver_cuda,
        vulkan_available: os == Os::Windows && has_windows_vulkan(probe),
        metal: os == Os::MacOs && arch == Arch::Aarch64,
    }
}

fn detect_nvidia(probe: &dyn SystemProbe) -> (Vec<GpuInfo>, Option<CudaVersion>) {
    let gpus = probe
        .run(NVIDIA_SMI, &NVIDIA_SMI_GPU_QUERY)
        .map(|csv| parse_nvidia_smi_gpus(&csv))
        .unwrap_or_default();
    let driver_cuda = probe
        .run(NVIDIA_SMI, &[])
        .and_then(|header| parse_nvidia_smi_cuda(&header));
    (gpus, driver_cuda)
}

fn has_windows_vulkan(probe: &dyn SystemProbe) -> bool {
    probe
        .system_root()
        .is_some_and(|root| probe.file_exists(&root.join("System32").join("vulkan-1.dll")))
}

/// Sonda real del sistema. La app solo soporta Windows y macOS, x86_64 y aarch64:
/// cualquier otro valor se trata como Windows / x86_64.
pub struct NativeProbe;

impl SystemProbe for NativeProbe {
    fn os(&self) -> Os {
        match std::env::consts::OS {
            "macos" => Os::MacOs,
            _ => Os::Windows,
        }
    }

    fn arch(&self) -> Arch {
        match std::env::consts::ARCH {
            "aarch64" => Arch::Aarch64,
            _ => Arch::X86_64,
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn cpu_has(&self, feature: &str) -> bool {
        match feature {
            "avx2" => std::arch::is_x86_feature_detected!("avx2"),
            "avx512f" => std::arch::is_x86_feature_detected!("avx512f"),
            _ => false,
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn cpu_has(&self, _feature: &str) -> bool {
        false
    }

    fn run(&self, program: &str, args: &[&str]) -> Option<String> {
        run_with_timeout(program, args, COMMAND_TIMEOUT)
    }

    fn file_exists(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn system_root(&self) -> Option<PathBuf> {
        std::env::var_os("SystemRoot").map(PathBuf::from)
    }
}

/// Ejecuta el programa y devuelve su salida si termina con código 0 antes del límite;
/// si no, lo mata y devuelve `None`.
fn run_with_timeout(program: &str, args: &[&str], timeout: Duration) -> Option<String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    hide_console(&mut command);
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    // Se lee en un hilo para que un pipe lleno no bloquee al hijo mientras se espera.
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).ok().map(|_| bytes)
    });
    let status = wait_or_kill(&mut child, timeout)?;
    let bytes = reader.join().ok()??;
    status
        .success()
        .then(|| String::from_utf8_lossy(&bytes).into_owned())
}

fn wait_or_kill(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if started.elapsed() < timeout => thread::sleep(POLL_INTERVAL),
            _ => {
                // Si el kill falla el proceso ya terminó; `wait` lo recoge igualmente.
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::test_support::FakeProbe;
    use std::time::Instant;

    const SMI_HEADER: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/runtime/nvidia_smi_header.txt"
    ));
    const GPU_ARGS: [&str; 2] = [
        "--query-gpu=name,memory.total",
        "--format=csv,noheader,nounits",
    ];

    fn gpu(name: &str, vram_mib: Option<u64>) -> GpuInfo {
        GpuInfo {
            name: name.to_string(),
            vram_mib,
        }
    }

    fn system_root() -> PathBuf {
        PathBuf::from("sysroot")
    }

    fn vulkan_dll() -> PathBuf {
        system_root().join("System32").join("vulkan-1.dll")
    }

    fn windows_probe() -> FakeProbe {
        FakeProbe::new(Os::Windows, Arch::X86_64)
            .with_cpu(&["avx2"])
            .with_system_root(system_root())
    }

    fn nvidia_probe() -> FakeProbe {
        windows_probe()
            .with_output("nvidia-smi", &GPU_ARGS, "NVIDIA GeForce RTX 4070, 12282\n")
            .with_output("nvidia-smi", &[], SMI_HEADER)
    }

    #[test]
    fn parse_gpus_una() {
        assert_eq!(
            parse_nvidia_smi_gpus("NVIDIA GeForce RTX 4070, 12282\n"),
            vec![gpu("NVIDIA GeForce RTX 4070", Some(12282))]
        );
    }

    #[test]
    fn parse_gpus_varias_y_espacios() {
        let csv = "  NVIDIA GeForce RTX 4070 ,  12282 \r\n NVIDIA T4,15360\r\n";
        assert_eq!(
            parse_nvidia_smi_gpus(csv),
            vec![
                gpu("NVIDIA GeForce RTX 4070", Some(12282)),
                gpu("NVIDIA T4", Some(15360))
            ]
        );
    }

    #[test]
    fn parse_gpus_vram_no_numerica() {
        assert_eq!(
            parse_nvidia_smi_gpus("Tesla X, [N/A]"),
            vec![gpu("Tesla X", None)]
        );
    }

    #[test]
    fn parse_gpus_vacio() {
        assert_eq!(parse_nvidia_smi_gpus(""), vec![]);
    }

    #[test]
    fn parse_cuda_cabecera_real() {
        assert_eq!(
            parse_nvidia_smi_cuda(SMI_HEADER),
            Some(CudaVersion {
                major: 13,
                minor: 0
            })
        );
    }

    #[test]
    fn parse_cuda_ausente_o_raro() {
        for header in ["", "CUDA Version: N/A", "CUDA Version: 12"] {
            assert_eq!(parse_nvidia_smi_cuda(header), None, "{header:?}");
        }
    }

    #[test]
    fn detect_windows_nvidia_vulkan() {
        let probe = nvidia_probe().with_file(vulkan_dll());
        assert_eq!(
            detect(&probe),
            HardwareInfo {
                os: Os::Windows,
                arch: Arch::X86_64,
                avx2: true,
                avx512: false,
                gpus: vec![gpu("NVIDIA GeForce RTX 4070", Some(12282))],
                driver_cuda: Some(CudaVersion {
                    major: 13,
                    minor: 0
                }),
                vulkan_available: true,
                metal: false,
            }
        );
    }

    #[test]
    fn detect_sin_nvidia_smi() {
        let hw = detect(&windows_probe());
        assert_eq!(hw.gpus, vec![]);
        assert_eq!(hw.driver_cuda, None);
    }

    #[test]
    fn detect_sin_vulkan() {
        assert!(!detect(&nvidia_probe()).vulkan_available);
    }

    #[test]
    fn detect_sin_system_root() {
        let probe = FakeProbe::new(Os::Windows, Arch::X86_64).with_file(vulkan_dll());
        assert!(!detect(&probe).vulkan_available);
    }

    #[test]
    fn detect_macos_arm64() {
        let probe = FakeProbe::new(Os::MacOs, Arch::Aarch64)
            .with_cpu(&["avx2"])
            .with_system_root(system_root())
            .with_file(vulkan_dll());
        let hw = detect(&probe);
        assert!(hw.metal);
        assert!(!hw.avx2);
        assert!(!hw.vulkan_available);
        assert_eq!(probe.run_count(), 0);
    }

    #[test]
    fn detect_macos_x64() {
        let probe = FakeProbe::new(Os::MacOs, Arch::X86_64).with_cpu(&["avx2"]);
        let hw = detect(&probe);
        assert!(!hw.metal);
        assert!(hw.avx2);
    }

    #[test]
    fn native_probe_no_falla() {
        let hw = detect(&NativeProbe);
        let expected_os = if std::env::consts::OS == "macos" {
            Os::MacOs
        } else {
            Os::Windows
        };
        let expected_arch = if std::env::consts::ARCH == "aarch64" {
            Arch::Aarch64
        } else {
            Arch::X86_64
        };
        assert_eq!(hw.os, expected_os);
        assert_eq!(hw.arch, expected_arch);
    }

    const SHORT_TIMEOUT: Duration = Duration::from_millis(300);

    #[test]
    fn run_con_limite_devuelve_la_salida() {
        let output = run_with_timeout("cargo", &["--version"], Duration::from_secs(30));
        assert!(output.is_some_and(|text| text.starts_with("cargo ")));
    }

    #[test]
    fn run_con_limite_aborta_programa_lento() {
        // `ping` y `sleep` existen de serie en Windows y macOS; `ping` solo toca el loopback.
        let (program, args): (&str, &[&str]) = if cfg!(windows) {
            ("ping", &["-n", "30", "127.0.0.1"])
        } else {
            ("sleep", &["30"])
        };
        let started = Instant::now();
        assert_eq!(run_with_timeout(program, args, SHORT_TIMEOUT), None);
        assert!(started.elapsed() < Duration::from_secs(20));
    }

    #[test]
    fn run_con_limite_programa_inexistente() {
        assert_eq!(
            run_with_timeout("programa-que-no-existe-app-llama", &[], SHORT_TIMEOUT),
            None
        );
    }
}
