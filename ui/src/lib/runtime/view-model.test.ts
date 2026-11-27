import { describe, expect, it } from "vitest";
import type {
  ActiveRuntime,
  Backend,
  HardwareInfo,
  ProgressEvent,
  RuntimeStatus,
  UpdateInfo,
} from "./types";
import {
  backendHint,
  backendOptions,
  describeHardware,
  describeStatus,
  errorMessage,
  externalBinaryFilters,
  primaryAction,
  progressLabel,
  progressPercent,
} from "./view-model";

const CPU: Backend = { Cpu: { isa: "Avx2" } };
const CUDA_13: Backend = { Cuda: { major: 13, minor: 5 } };
const CUDA_12: Backend = { Cuda: { major: 12, minor: 4 } };

const NVIDIA: HardwareInfo = {
  os: "Windows",
  arch: "X86_64",
  avx2: true,
  avx512: false,
  gpus: [{ name: "NVIDIA GeForce RTX 4070", vram_mib: 12282 }],
  driver_cuda: { major: 13, minor: 5 },
  vulkan_available: true,
  metal: false,
};

const MAC: HardwareInfo = {
  os: "MacOs",
  arch: "Aarch64",
  avx2: false,
  avx512: false,
  gpus: [],
  driver_cuda: null,
  vulkan_available: false,
  metal: true,
};

function managed(build: number, backend: Backend): RuntimeStatus {
  const active: ActiveRuntime = {
    source: { Managed: { tag: `b${build}` } },
    backend,
    server_path: "C:\\llama\\llama-server.exe",
    version: { build, commit: "abc" },
  };
  return { Ready: active };
}

const EXTERNAL: RuntimeStatus = {
  Ready: {
    source: "External",
    backend: null,
    server_path: "D:\\bin\\llama-server.exe",
    version: { build: 12000, commit: "def" },
  },
};

function update(overrides: Partial<UpdateInfo> = {}): UpdateInfo {
  return {
    installed: 12780,
    latest: 12780,
    latest_tag: "b12780",
    available_backends: ["Vulkan", CPU],
    update_available: false,
    ...overrides,
  };
}

describe("describeHardware", () => {
  it("describe_hardware_nvidia", () => {
    const text = describeHardware(NVIDIA);

    expect(text).toContain("AVX2");
    expect(text).toContain("NVIDIA GeForce RTX 4070 (12 GB)");
    expect(text).toContain("Vulkan");
    expect(text).not.toContain("AVX512");
  });

  it("describe_hardware_mac", () => {
    expect(describeHardware(MAC)).toContain("Apple Silicon (Metal)");
  });
});

describe("describeStatus", () => {
  it("describe_status_no_instalado", () => {
    expect(describeStatus("NotInstalled")).toBe("No instalado");
  });

  it("describe_status_listo_gestionado", () => {
    expect(describeStatus(managed(12780, CUDA_13))).toBe("b12780 · CUDA 13.5 · gestionado");
  });

  it("describe_status_externo", () => {
    const text = describeStatus(EXTERNAL);

    expect(text).toContain("b12000");
    expect(text).toContain("externo");
    expect(text).toContain("D:\\bin\\llama-server.exe");
    for (const name of ["CPU", "CUDA", "Vulkan", "Metal"]) {
      expect(text).not.toContain(name);
    }
  });

  it("describe_status_roto", () => {
    const text = describeStatus({ Broken: { path: "C:\\llama\\llama-server.exe" } });

    expect(text).toContain("no se encuentra");
    expect(text).toContain("C:\\llama\\llama-server.exe");
  });
});

describe("primaryAction", () => {
  const cpuActive = managed(12780, CPU);

  it("accion_instalar", () => {
    expect(primaryAction("NotInstalled", null, "auto")).toEqual({
      label: "Instalar",
      kind: "install",
      enabled: true,
    });
  });

  it("accion_instalar_roto", () => {
    expect(primaryAction({ Broken: { path: "x" } }, update(), "cpu")).toEqual({
      label: "Instalar",
      kind: "install",
      enabled: true,
    });
  });

  it("accion_actualizar", () => {
    const info = update({ installed: 12779, update_available: true });

    expect(primaryAction(managed(12779, CPU), info, "cpu")).toEqual({
      label: "Actualizar a b12780",
      kind: "install",
      enabled: true,
    });
  });

  it("accion_actualizar_otro_backend", () => {
    const info = update({ installed: 12779, update_available: true });

    expect(primaryAction(managed(12779, CPU), info, "vulkan")).toEqual({
      label: "Actualizar a b12780",
      kind: "install",
      enabled: true,
    });
  });

  it("accion_al_dia", () => {
    expect(primaryAction(cpuActive, update(), "cpu")).toEqual({
      label: "Al día",
      kind: "none",
      enabled: false,
    });
  });

  it("accion_sin_info_update", () => {
    expect(primaryAction(cpuActive, null, "vulkan")).toEqual({
      label: "Buscar actualizaciones",
      kind: "check",
      enabled: true,
    });
  });

  it("accion_reinstalar_otro_backend", () => {
    expect(primaryAction(cpuActive, update(), "vulkan")).toEqual({
      label: "Instalar Vulkan",
      kind: "install",
      enabled: true,
    });
  });

  it("accion_auto_compara_con_primero", () => {
    const info = update({ available_backends: [CUDA_13, "Vulkan", CPU] });

    expect(primaryAction(cpuActive, info, "auto")).toEqual({
      label: "Instalar CUDA 13.5",
      kind: "install",
      enabled: true,
    });
    expect(primaryAction(managed(12780, CUDA_13), info, "auto")).toEqual({
      label: "Al día",
      kind: "none",
      enabled: false,
    });
  });

  it("accion_cuda_otra_version", () => {
    const info = update({ available_backends: [CUDA_13, CPU] });

    expect(primaryAction(managed(12780, CUDA_12), info, "cuda")).toEqual({
      label: "Instalar CUDA 13.5",
      kind: "install",
      enabled: true,
    });
  });

  it("accion_externo_ofrece_instalar", () => {
    expect(primaryAction(EXTERNAL, update(), "auto")).toEqual({
      label: "Instalar Vulkan",
      kind: "install",
      enabled: true,
    });
  });

  it("accion_backend_no_disponible_usa_auto", () => {
    const info = update({ available_backends: [CPU] });

    expect(primaryAction(cpuActive, info, "vulkan")).toEqual({
      label: "Al día",
      kind: "none",
      enabled: false,
    });
  });
});

describe("backendOptions", () => {
  it("opciones_backend", () => {
    const options = backendOptions(update({ available_backends: [CUDA_13, "Vulkan", CPU] }));

    expect(options.map((option) => option.value)).toEqual(["auto", "cuda", "vulkan", "cpu"]);
    expect(options.map((option) => option.label)).toEqual([
      "Automático",
      "CUDA 13.5",
      "Vulkan",
      "CPU",
    ]);
  });

  it("opciones_backend_sin_consultar", () => {
    expect(backendOptions(null).map((option) => option.value)).toEqual(["auto"]);
  });
});

describe("backendHint", () => {
  it("backend_hint_sin_consultar", () => {
    expect(backendHint(null)).toBe('Pulsa "Buscar actualizaciones" para ver los backends disponibles.');
  });

  it("backend_hint_tras_consultar", () => {
    expect(backendHint(update({ available_backends: [CUDA_13, "Vulkan", CPU] }))).toBeNull();
    expect(
      backendHint(update({ available_backends: [CPU], update_available: true })),
    ).toBeNull();
  });
});

describe("progreso", () => {
  const fetching = (artifact: "main" | "cudart", done: number, total: number): ProgressEvent => ({
    phase: { kind: "fetching", artifact },
    done,
    total,
  });

  it("progreso_etiquetas", () => {
    expect(progressLabel(fetching("main", 42, 100))).toBe("Descargando llama.cpp… 42 %");
    expect(progressLabel(fetching("cudart", 1, 4))).toBe("Descargando DLL de CUDA… 25 %");
    expect(progressLabel({ phase: { kind: "extracting" }, done: 0, total: 0 })).toBe("Extrayendo…");
    expect(progressLabel({ phase: { kind: "verifying" }, done: 0, total: 0 })).toBe(
      "Verificando…",
    );
    expect(progressLabel(fetching("main", 0, 0))).toBe("Descargando llama.cpp…");
  });

  it("progreso_porcentaje", () => {
    expect(progressPercent(fetching("main", 42, 100))).toBe(42);
    expect(progressPercent(fetching("main", 5, 0))).toBeNull();
  });
});

describe("errorMessage", () => {
  it("mensaje_error_rate_limit", () => {
    const message = errorMessage({
      code: "RateLimited",
      message: "Se alcanzó el límite de GitHub. Inténtalo más tarde.",
    });

    expect(message).toContain("Inténtalo más tarde");
  });

  it("mensaje_error_desconocido", () => {
    expect(errorMessage(new Error("ipc"))).toBe("Ha ocurrido un error inesperado.");
    expect(errorMessage(undefined)).toBe("Ha ocurrido un error inesperado.");
  });
});

describe("externalBinaryFilters", () => {
  it("filtro_exe_solo_en_windows", () => {
    expect(externalBinaryFilters("Windows")).toEqual([
      { name: "llama-server", extensions: ["exe"] },
    ]);
    expect(externalBinaryFilters("MacOs")).toBeUndefined();
    expect(externalBinaryFilters(undefined)).toBeUndefined();
  });
});
