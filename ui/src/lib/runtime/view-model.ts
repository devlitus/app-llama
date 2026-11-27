import type {
  Backend,
  BackendChoice,
  ErrorPayload,
  HardwareInfo,
  Os,
  ProgressEvent,
  RuntimeStatus,
  UpdateInfo,
} from "./types";

export type ActionKind = "check" | "install" | "none";

export interface PrimaryAction {
  label: string;
  kind: ActionKind;
  enabled: boolean;
}

export interface BackendOption {
  value: BackendChoice;
  label: string;
}

export interface FileFilter {
  name: string;
  extensions: string[];
}


const UNKNOWN_ERROR = "Ha ocurrido un error inesperado.";
const GIBIBYTE_IN_MIB = 1024;

function backendKind(backend: Backend): Exclude<BackendChoice, "auto"> {
  if (backend === "Vulkan") return "vulkan";
  if (backend === "Metal") return "metal";
  return "Cpu" in backend ? "cpu" : "cuda";
}

function backendLabel(backend: Backend): string {
  if (backend === "Vulkan" || backend === "Metal") return backend;
  if ("Cuda" in backend) return `CUDA ${backend.Cuda.major}.${backend.Cuda.minor}`;
  return "CPU";
}

function sameBackend(left: Backend, right: Backend): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function describeGpu({ name, vram_mib }: HardwareInfo["gpus"][number]): string {
  return vram_mib === null ? name : `${name} (${Math.round(vram_mib / GIBIBYTE_IN_MIB)} GB)`;
}

export function describeHardware(hardware: HardwareInfo): string {
  const parts = [hardware.avx512 ? "AVX512" : hardware.avx2 ? "AVX2" : "CPU estándar"];
  parts.push(...hardware.gpus.map(describeGpu));
  if (hardware.driver_cuda) {
    parts.push(`CUDA ${hardware.driver_cuda.major}.${hardware.driver_cuda.minor}`);
  }
  if (hardware.vulkan_available) parts.push("Vulkan");
  if (hardware.metal) parts.push("Apple Silicon (Metal)");
  return parts.join(" · ");
}

export function describeStatus(status: RuntimeStatus): string {
  if (status === "NotInstalled") return "No instalado";
  if ("Broken" in status) {
    return `El binario de llama.cpp no se encuentra: ${status.Broken.path}`;
  }
  const { source, backend, version, server_path } = status.Ready;
  const build = `b${version.build}`;
  if (source === "External") return `${build} · externo · ${server_path}`;
  const backendPart = backend ? ` · ${backendLabel(backend)}` : "";
  return `${build}${backendPart} · gestionado`;
}

function effectiveBackend(update: UpdateInfo, choice: BackendChoice): Backend | undefined {
  const [first] = update.available_backends;
  if (choice === "auto") return first;
  return update.available_backends.find((backend) => backendKind(backend) === choice) ?? first;
}

function installBackendAction(
  status: RuntimeStatus,
  update: UpdateInfo,
  choice: BackendChoice,
): PrimaryAction {
  const target = effectiveBackend(update, choice);
  const active = typeof status === "object" && "Ready" in status ? status.Ready : null;
  const differs =
    target !== undefined &&
    (active?.backend == null || !sameBackend(active.backend, target));
  return differs
    ? { label: `Instalar ${backendLabel(target)}`, kind: "install", enabled: true }
    : { label: "Al día", kind: "none", enabled: false };
}

export function primaryAction(
  status: RuntimeStatus,
  update: UpdateInfo | null,
  choice: BackendChoice,
): PrimaryAction {
  if (status === "NotInstalled" || "Broken" in status) return { label: "Instalar", kind: "install", enabled: true };
  if (update === null) return { label: "Buscar actualizaciones", kind: "check", enabled: true };
  if (update.update_available) {
    return { label: `Actualizar a b${update.latest}`, kind: "install", enabled: true };
  }
  return installBackendAction(status, update, choice);
}

export function backendOptions(update: UpdateInfo | null): BackendOption[] {
  const options: BackendOption[] = [{ value: "auto", label: "Automático" }];
  for (const backend of update?.available_backends ?? []) {
    const value = backendKind(backend);
    if (!options.some((option) => option.value === value)) {
      options.push({ value, label: backendLabel(backend) });
    }
  }
  return options;
}

export function backendHint(update: UpdateInfo | null): string | null {
  return update === null
    ? 'Pulsa "Buscar actualizaciones" para ver los backends disponibles.'
    : null;
}

export function progressPercent(event: ProgressEvent): number | null {
  if (event.total <= 0) return null;
  return Math.floor((event.done * 100) / event.total);
}

function withPercent(text: string, percent: number | null): string {
  return percent === null ? `${text}…` : `${text}… ${percent} %`;
}

export function progressLabel(event: ProgressEvent): string {
  switch (event.phase.kind) {
    case "fetching": {
      const subject = event.phase.artifact === "cudart" ? "DLL de CUDA" : "llama.cpp";
      return withPercent(`Descargando ${subject}`, progressPercent(event));
    }
    case "extracting":
      return "Extrayendo…";
    case "verifying":
      return "Verificando…";
    case "done":
      return "Instalación completada";
  }
}

function isErrorPayload(error: unknown): error is ErrorPayload {
  return (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error &&
    typeof error.message === "string"
  );
}

export function errorMessage(error: unknown): string {
  return isErrorPayload(error) ? error.message : UNKNOWN_ERROR;
}

export function externalBinaryFilters(os: Os | undefined): FileFilter[] | undefined {
  return os === "Windows" ? [{ name: "llama-server", extensions: ["exe"] }] : undefined;
}
