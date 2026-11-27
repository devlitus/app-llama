import { render, screen } from "@testing-library/svelte";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import Settings from "./Settings.svelte";

afterEach(clearMocks);

function mockCommands(appInfo: () => unknown): void {
  mockIPC((cmd) => {
    switch (cmd) {
      case "app_info":
        return appInfo();
      case "runtime_hardware":
        return {
          os: "Windows",
          arch: "X86_64",
          avx2: true,
          avx512: false,
          gpus: [],
          driver_cuda: null,
          vulkan_available: false,
          metal: false,
          ram_bytes: null,
        };
      default:
        return "NotInstalled";
    }
  });
}

describe("Settings", () => {
  it("ajustes_muestra_version", async () => {
    mockCommands(() => ({ name: "app-llama", version: "0.1.0" }));

    render(Settings);

    expect(await screen.findByText("Versión 0.1.0")).toBeInTheDocument();
  });

  it("ajustes_version_no_disponible", async () => {
    mockCommands(() => {
      throw new Error("ipc");
    });

    render(Settings);

    expect(await screen.findByText("Versión no disponible")).toBeInTheDocument();
  });

  it("ajustes_incluye_seccion_llama_cpp", async () => {
    mockCommands(() => ({ name: "app-llama", version: "0.1.0" }));

    render(Settings);

    expect(await screen.findByRole("heading", { level: 2, name: "llama.cpp" })).toBeInTheDocument();
  });
});
