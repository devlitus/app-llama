import { emit } from "@tauri-apps/api/event";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { cleanup, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { DownloadSnapshot, UnitState } from "../lib/downloads/types";
import Downloads from "./Downloads.svelte";

afterEach(() => {
  cleanup();
  clearMocks();
});

function snapshot(id: number, state: UnitState = "Queued"): DownloadSnapshot {
  return {
    id,
    version: 1,
    repo_id: "ana/modelo-GGUF",
    revision: "9217aaaa",
    display_name: `modelo-${id}`,
    state,
    total_bytes: 1024,
    files: [],
  };
}

describe("Downloads", () => {
  it("vista_lista_inicial_y_eventos", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    mockIPC((cmd) => (cmd === "downloads_list" ? [snapshot(1)] : undefined), {
      shouldMockEvents: true,
    });
    render(Downloads);

    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Descargas");
    expect(await screen.findByText("modelo-1")).toBeInTheDocument();

    await emit("downloads://state", snapshot(2));

    await waitFor(() => expect(screen.getAllByRole("listitem")).toHaveLength(2));
    const logged = [...warn.mock.calls, ...error.mock.calls].flat().join(" ");
    expect(logged).not.toContain("each_key_duplicate");
    warn.mockRestore();
    error.mockRestore();
  });

  it("vista_sin_descargas", async () => {
    mockIPC(() => [], { shouldMockEvents: true });
    render(Downloads);

    expect(await screen.findByText("No hay descargas")).toBeInTheDocument();
  });
});
