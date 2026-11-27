import { render, screen } from "@testing-library/svelte";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import Settings from "./Settings.svelte";

afterEach(clearMocks);

describe("Settings", () => {
  it("ajustes_muestra_version", async () => {
    mockIPC(() => ({ name: "app-llama", version: "0.1.0" }));

    render(Settings);

    expect(await screen.findByText("Versión 0.1.0")).toBeInTheDocument();
  });

  it("ajustes_version_no_disponible", async () => {
    mockIPC(() => {
      throw new Error("ipc");
    });

    render(Settings);

    expect(await screen.findByText("Versión no disponible")).toBeInTheDocument();
  });
});
