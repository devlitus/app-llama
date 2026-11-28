import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import { afterEach, describe, expect, it } from "vitest";
import RuntimeSettings from "./RuntimeSettings.svelte";
import type { HardwareInfo, UpdateInfo } from "./types";

afterEach(clearMocks);

const HARDWARE: HardwareInfo = {
  os: "Windows",
  arch: "X86_64",
  avx2: true,
  avx512: false,
  gpus: [{ name: "NVIDIA GeForce RTX 4070", vram_mib: 12282 }],
  driver_cuda: { major: 13, minor: 5 },
  vulkan_available: true,
  metal: false,
  ram_bytes: 34359738368,
};

function expectEnabled(name: string): Promise<void> {
  return waitFor(() => expect(screen.getByRole("button", { name })).toBeEnabled());
}

describe("RuntimeSettings", () => {
  it("runtime_settings_muestra_estado", async () => {
    mockIPC((cmd) => (cmd === "runtime_hardware" ? HARDWARE : "NotInstalled"));

    render(RuntimeSettings);

    expect(await screen.findByText("No instalado")).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Instalar" })).toBeEnabled();
    expect(await screen.findByText(/NVIDIA GeForce RTX 4070 \(12 GB\)/)).toBeInTheDocument();
  });

  it("runtime_settings_error_backend", async () => {
    mockIPC((cmd) => {
      if (cmd === "runtime_hardware") return HARDWARE;
      throw { code: "Io", message: "No se pudo leer el estado del runtime" };
    });

    render(RuntimeSettings);

    expect(await screen.findByText("No se pudo leer el estado del runtime")).toBeInTheDocument();
  });

  it("runtime_settings_busca_actualizaciones", async () => {
    const info: UpdateInfo = {
      installed: null,
      latest: 12780,
      latest_tag: "b12780",
      available_backends: ["Vulkan"],
      update_available: true,
    };
    mockIPC((cmd) => {
      if (cmd === "runtime_hardware") return HARDWARE;
      if (cmd === "runtime_check_update") return info;
      return {
        Ready: {
          source: "External",
          backend: null,
          server_path: "D:\\bin\\llama-server.exe",
          version: { build: 12000, commit: "def" },
        },
      };
    });

    render(RuntimeSettings);
    await fireEvent.click(await screen.findByRole("button", { name: "Buscar actualizaciones" }));

    await expectEnabled("Actualizar a b12780");
  });

  it("runtime_settings_instalar_y_cancelar", async () => {
    const calls: string[] = [];
    let rejectInstall: (reason: unknown) => void = () => {};
    mockIPC(
      (cmd) => {
        calls.push(cmd);
        if (cmd === "runtime_hardware") return HARDWARE;
        if (cmd === "runtime_install") {
          return new Promise((_resolve, reject) => (rejectInstall = reject));
        }
        return "NotInstalled";
      },
      { shouldMockEvents: true },
    );
    const progress = (done: number) => ({
      phase: { kind: "fetching", artifact: "main" },
      done,
      total: 100,
    });

    render(RuntimeSettings);
    await fireEvent.click(await screen.findByRole("button", { name: "Instalar" }));
    await waitFor(() => expect(calls).toContain("runtime_install"));
    await emit("runtime://progress", progress(42));

    expect(await screen.findByText("Descargando llama.cpp… 42 %")).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(calls).toContain("runtime_cancel_install");

    rejectInstall({ code: "Cancelled", message: "Instalación cancelada" });

    expect(await screen.findByText("Instalación cancelada")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancelar" })).not.toBeInTheDocument();
    await expectEnabled("Instalar");
    await emit("runtime://progress", progress(77));
    expect(screen.queryByText(/77 %/)).not.toBeInTheDocument();
  });

  it("runtime_settings_busy_bloquea_acciones", async () => {
    let answerCheck: (info: UpdateInfo) => void = () => {};
    mockIPC((cmd) => {
      if (cmd === "runtime_hardware") return HARDWARE;
      if (cmd === "runtime_check_update") return new Promise((resolve) => (answerCheck = resolve));
      return {
        Ready: {
          source: "External",
          backend: null,
          server_path: "D:\\bin\\llama-server.exe",
          version: { build: 12000, commit: "def" },
        },
      };
    });

    render(RuntimeSettings);
    await fireEvent.click(await screen.findByRole("button", { name: "Buscar actualizaciones" }));

    expect(screen.getByRole("button", { name: "Usar binario existente…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Buscar actualizaciones" })).toBeDisabled();
    answerCheck({
      installed: null,
      latest: 12780,
      latest_tag: "b12780",
      available_backends: ["Vulkan"],
      update_available: true,
    });
    await expectEnabled("Actualizar a b12780");
    await expectEnabled("Usar binario existente…");
  });

  it("runtime_settings_aviso_backends", async () => {
    const hint = 'Pulsa "Buscar actualizaciones" para ver los backends disponibles.';
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      if (cmd === "runtime_hardware") return HARDWARE;
      if (cmd === "runtime_check_update") {
        return {
          installed: null,
          latest: 12000,
          latest_tag: "b12000",
          available_backends: [{ Cuda: { major: 13, minor: 5 } }, "Vulkan", { Cpu: { isa: "Avx2" } }],
          update_available: false,
        } satisfies UpdateInfo;
      }
      return {
        Ready: {
          source: "External",
          backend: null,
          server_path: "D:\\bin\\llama-server.exe",
          version: { build: 12000, commit: "def" },
        },
      };
    });
    const optionNames = () => screen.getAllByRole("option").map((option) => option.textContent);

    render(RuntimeSettings);
    expect(await screen.findByText(hint)).toBeInTheDocument();
    expect(optionNames()).toEqual(["Automático"]);
    expect(screen.getByRole("combobox")).toHaveAccessibleDescription(hint);
    expect(calls).not.toContain("runtime_check_update");

    await fireEvent.click(await screen.findByRole("button", { name: "Buscar actualizaciones" }));

    await waitFor(() => expect(screen.queryByText(hint)).not.toBeInTheDocument());
    expect(optionNames()).toEqual(["Automático", "CUDA 13.5", "Vulkan", "CPU"]);
    expect(calls.filter((cmd) => cmd === "runtime_check_update")).toHaveLength(1);
  });
});
