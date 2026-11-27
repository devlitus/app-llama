// Espejo de la serialización serde de Rust (sin `rename_all`, salvo `ProgressEvent`). La forma
// exacta está fijada por `json_hardware_status_y_update_para_la_ui` en `src-tauri`.

export type Os = "Windows" | "MacOs";
export type Arch = "X86_64" | "Aarch64";
export type CpuIsa = "Generic" | "Avx" | "Avx2" | "Avx512";

export type Backend =
  | { Cpu: { isa: CpuIsa } }
  | { Cuda: { major: number; minor: number } }
  | "Vulkan"
  | "Metal";

export type BackendChoice = "auto" | "cpu" | "cuda" | "vulkan" | "metal";

export interface GpuInfo {
  name: string;
  vram_mib: number | null;
}

export interface HardwareInfo {
  os: Os;
  arch: Arch;
  avx2: boolean;
  avx512: boolean;
  gpus: GpuInfo[];
  driver_cuda: { major: number; minor: number } | null;
  vulkan_available: boolean;
  metal: boolean;
}

export type RuntimeSource = { Managed: { tag: string } } | "External";

export interface ActiveRuntime {
  source: RuntimeSource;
  /** `null` en los binarios externos: no se sabe con qué backend se compilaron. */
  backend: Backend | null;
  server_path: string;
  version: { build: number; commit: string };
}

export type RuntimeStatus = "NotInstalled" | { Ready: ActiveRuntime } | { Broken: { path: string } };

export interface UpdateInfo {
  installed: number | null;
  latest: number;
  latest_tag: string;
  available_backends: Backend[];
  update_available: boolean;
}

export type ProgressPhase =
  | { kind: "fetching"; artifact: "main" | "cudart" }
  | { kind: "extracting" }
  | { kind: "verifying" }
  | { kind: "done" };

export interface ProgressEvent {
  phase: ProgressPhase;
  done: number;
  total: number;
}

export interface ErrorPayload {
  code: string;
  message: string;
}
