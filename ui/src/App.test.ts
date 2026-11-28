import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import App from "./App.svelte";
import { SECTIONS } from "./lib/navigation";

afterEach(() => {
  // Desmontar antes: Downloads se desuscribe al salir y necesita el mock de eventos vivo.
  cleanup();
  clearMocks();
});

function navigationButtons(): HTMLElement[] {
  return within(screen.getByRole("navigation", { name: "Secciones" })).getAllByRole("button");
}

describe("App", () => {
  it("muestra_seis_botones_de_navegacion", () => {
    render(App);
    expect(navigationButtons().map((b) => b.textContent?.trim())).toEqual(
      SECTIONS.map((s) => s.label),
    );
  });

  it("seccion_inicial_explorar", () => {
    render(App);
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Explorar");
    expect(screen.getByRole("button", { name: "Explorar" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("navega_a_cada_seccion", async () => {
    render(App);
    for (const { label } of SECTIONS) {
      await fireEvent.click(screen.getByRole("button", { name: label }));
      expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(label);
      const current = navigationButtons().filter((b) => b.getAttribute("aria-current") === "page");
      expect(current).toHaveLength(1);
      expect(current[0]).toHaveTextContent(label);
    }
  });

  it("descargar_desde_la_ficha_navega_a_descargas", async () => {
    const part = { path: "m-Q4.gguf", size: 2_147_483_648, sha256: null };
    const detail = {
      info: { id: "a/open", gated: "Open", downloads: 1, likes: 1, gguf: null, license: null, sha: null },
      ctx: 4096,
      budget: { gpu_bytes: null, ram_bytes: null, unified: false, usable_percent: 80 },
      variants: [
        {
          variant: { name: "Q4", quant: "Q4", parts: [part], total_bytes: part.size, complete: true },
          estimate: { bytes: 1, kv_included: true },
          fit: "Gpu",
          recommended: false,
        },
      ],
      revision: "9217a1b2c3d4e5f60718293a4b5c6d7e8f901234",
    };
    const found = {
      items: [
        {
          model: {
            id: "a/open",
            downloads: 1,
            likes: 1,
            gated: "Open",
            last_modified: "2026-11-12T02:15:52.000Z",
            pipeline_tag: null,
            gguf: null,
          },
          hint: null,
        },
      ],
      next: null,
    };
    const responses: Record<string, unknown> = {
      hf_search: found,
      hf_model_detail: detail,
      downloads_enqueue_hf: 7,
      downloads_list: [],
    };
    mockIPC((cmd) => responses[cmd], { shouldMockEvents: true });
    render(App);

    await fireEvent.input(screen.getByRole("searchbox"), { target: { value: "llama" } });
    await fireEvent.click(screen.getByRole("button", { name: "Buscar" }));
    await fireEvent.click(await screen.findByRole("button", { name: /a\/open/ }));
    await fireEvent.click(await screen.findByRole("button", { name: "Descargar" }));

    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Descargas"),
    );
    expect(
      within(screen.getByRole("navigation", { name: "Secciones" })).getByRole("button", {
        name: "Descargas",
      }),
    ).toHaveAttribute("aria-current", "page");
  });

  it("click_en_la_activa_no_cambia", async () => {
    render(App);
    await fireEvent.click(screen.getByRole("button", { name: "Explorar" }));
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Explorar");
  });
});
