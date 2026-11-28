import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import type { FitHint, Gated, SearchItem, SearchPage } from "../lib/hf/types";
import Explore from "./Explore.svelte";

afterEach(clearMocks);

type Call = { cmd: string; args: unknown };

function item(id: string, gated: Gated = "Open", hint: FitHint | null = null): SearchItem {
  return {
    model: {
      id,
      downloads: 12_345,
      likes: 7,
      gated,
      last_modified: "2026-11-12T02:15:52.000Z",
      pipeline_tag: null,
      gguf: null,
    },
    hint,
  };
}

function page(items: SearchItem[], next: string | null = null): SearchPage {
  return { items, next };
}

function record(handler: (cmd: string, args: unknown) => unknown): Call[] {
  const calls: Call[] = [];
  mockIPC((cmd, args) => {
    calls.push({ cmd, args });
    return handler(cmd, args);
  });
  return calls;
}

function callsTo(calls: Call[], cmd: string): Call[] {
  return calls.filter((call) => call.cmd === cmd);
}

async function search(text: string): Promise<void> {
  await fireEvent.input(screen.getByRole("searchbox"), { target: { value: text } });
  await fireEvent.click(screen.getByRole("button", { name: "Buscar" }));
}

describe("Explore", () => {
  it("explorar_renders_results", async () => {
    record(() => page([item("a/open"), item("b/closed", "Manual")]));
    render(Explore);

    await search("llama");

    expect(await screen.findAllByRole("listitem")).toHaveLength(2);
    expect(screen.getAllByText("Restringido")).toHaveLength(1);
  });

  it("explorar_renders_fit_badges", async () => {
    const calls = record(() =>
      page([
        item("a/gpu", "Open", { estimate: { bytes: 6_481_006_131, kv_included: true }, fit: "Gpu" }),
        item("b/nofit", "Open", { estimate: { bytes: 52_761_967_679, kv_included: true }, fit: "NoFit" }),
        item("c/none"),
      ]),
    );
    render(Explore);

    await search("llama");

    const cards = await screen.findAllByRole("listitem");
    const gpu = within(cards[0]).getByText("≈ Cabe en GPU");
    expect(gpu).toHaveAttribute(
      "title",
      "Orientativa: estimada para una cuantización Q4_K_M típica (≈ 6,0 GiB)",
    );
    expect(within(cards[1]).getByText("≈ No cabe")).toBeInTheDocument();
    expect(within(cards[2]).queryByText(/≈/)).toBeNull();
    expect(callsTo(calls, "hf_search")).toHaveLength(1);
  });

  it("explorar_hint_without_fit_no_badge", async () => {
    record(() =>
      page([item("a/unknown", "Open", { estimate: { bytes: 6_481_006_131, kv_included: true }, fit: null })]),
    );
    render(Explore);

    await search("llama");

    const card = await screen.findByRole("listitem");
    expect(within(card).queryByText(/≈/)).toBeNull();
  });

  it("explorar_sort_change_replaces_results", async () => {
    const calls = record((_cmd, args) =>
      (args as { sort: string }).sort === "likes" ? page([item("likes/only")]) : page([item("downloads/only")]),
    );
    render(Explore);

    await search("llama");
    await screen.findByText("downloads/only");
    await fireEvent.change(screen.getByLabelText("Ordenar por"), { target: { value: "likes" } });

    expect(await screen.findByText("likes/only")).toBeInTheDocument();
    expect(screen.queryByText("downloads/only")).toBeNull();
    expect(callsTo(calls, "hf_search")[1].args).toEqual({ query: "llama", sort: "likes" });
  });

  it("explorar_load_more_appends", async () => {
    record((cmd) =>
      cmd === "hf_search"
        ? page([item("a/1"), item("a/2")], "https://huggingface.co/next")
        : page([item("b/1"), item("b/2")]),
    );
    render(Explore);

    await search("llama");
    await fireEvent.click(await screen.findByRole("button", { name: "Cargar más" }));

    await waitFor(() => expect(screen.getAllByRole("listitem")).toHaveLength(4));
    expect(screen.queryByRole("button", { name: "Cargar más" })).toBeNull();
  });

  it("explorar_load_more_no_double_request", async () => {
    const calls = record((cmd) =>
      cmd === "hf_search"
        ? page([item("a/1")], "https://huggingface.co/next")
        : new Promise(() => undefined),
    );
    render(Explore);

    await search("llama");
    const more = await screen.findByRole("button", { name: "Cargar más" });
    await fireEvent.click(more);
    await fireEvent.click(more);

    expect(callsTo(calls, "hf_search_next")).toHaveLength(1);
  });

  it("explorar_empty_state", async () => {
    record(() => page([]));
    render(Explore);

    await search("nada");

    expect(await screen.findByText("Sin resultados")).toBeInTheDocument();
  });

  it("explorar_shows_error", async () => {
    record(() => {
      throw { code: "RateLimited", message: "Hugging Face limita las peticiones; reintenta en 30 s" };
    });
    render(Explore);

    await search("llama");

    expect(await screen.findByText(/reintenta en 30 s/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reintentar" })).toBeInTheDocument();
  });

  it("explorar_retry_repeats_search", async () => {
    let failing = true;
    const calls = record(() => {
      if (failing) throw { code: "Network", message: "Sin conexión con Hugging Face" };
      return page([item("a/ok")]);
    });
    render(Explore);

    await search("llama");
    const retry = await screen.findByRole("button", { name: "Reintentar" });
    failing = false;
    await fireEvent.click(retry);

    expect(await screen.findByText("a/ok")).toBeInTheDocument();
    expect(screen.queryByText("Sin conexión con Hugging Face")).toBeNull();
    const searches = callsTo(calls, "hf_search");
    expect(searches).toHaveLength(2);
    expect(searches[1].args).toEqual(searches[0].args);
  });

  it("explorar_open_detail_and_back", async () => {
    const detail = {
      info: { id: "a/open", gated: "Open", downloads: 1, likes: 1, gguf: null, license: null },
      ctx: 4096,
      budget: { gpu_bytes: null, ram_bytes: null, unified: false, usable_percent: 80 },
      variants: [],
    };
    const calls = record((cmd) => (cmd === "hf_model_detail" ? detail : page([item("a/open"), item("b/other")])));
    render(Explore);

    await search("llama");
    await fireEvent.click(await screen.findByRole("button", { name: /a\/open/ }));

    await screen.findByRole("button", { name: "Volver" });
    expect(callsTo(calls, "hf_model_detail")[0].args).toEqual({ repoId: "a/open" });
    expect(screen.queryByRole("searchbox")).toBeNull();

    await fireEvent.click(screen.getByRole("button", { name: "Volver" }));

    expect(await screen.findAllByRole("listitem")).toHaveLength(2);
    expect(screen.getByRole("searchbox")).toHaveValue("llama");
    expect(callsTo(calls, "hf_search")).toHaveLength(1);
  });

  describe("alta de descarga desde la ficha", () => {
    const REVISION = "9217a1b2c3d4e5f60718293a4b5c6d7e8f901234";
    const PART = { path: "m-Q4.gguf", size: 2_147_483_648, sha256: null };
    const DETAIL = {
      info: { id: "a/open", gated: "Open", downloads: 1, likes: 1, gguf: null, license: null, sha: null },
      ctx: 4096,
      budget: { gpu_bytes: null, ram_bytes: null, unified: false, usable_percent: 80 },
      variants: [
        {
          variant: { name: "Q4", quant: "Q4", parts: [PART], total_bytes: PART.size, complete: true },
          estimate: { bytes: 1, kv_included: true },
          fit: "Gpu",
          recommended: false,
        },
      ],
      revision: REVISION,
    };

    async function openDetailAndDownload(handler: (cmd: string) => unknown, onNavigate: (id: string) => void) {
      const calls = record((cmd) => (cmd === "hf_model_detail" ? DETAIL : handler(cmd)));
      render(Explore, { onNavigate });
      await search("llama");
      await fireEvent.click(await screen.findByRole("button", { name: /a\/open/ }));
      await fireEvent.click(await screen.findByRole("button", { name: "Descargar" }));
      return calls;
    }

    it("ficha_descargar_encola_y_navega", async () => {
      const navigated: string[] = [];
      const calls = await openDetailAndDownload(
        (cmd) => (cmd === "downloads_enqueue_hf" ? 7 : page([item("a/open")])),
        (id) => navigated.push(id),
      );

      await waitFor(() => expect(navigated).toEqual(["downloads"]));
      expect(callsTo(calls, "downloads_enqueue_hf")[0].args).toEqual({
        request: { repo_id: "a/open", revision: REVISION, parts: [PART] },
      });
    });

    it("ficha_error_de_alta_se_muestra", async () => {
      const navigated: string[] = [];
      await openDetailAndDownload(
        (cmd) => {
          if (cmd === "downloads_enqueue_hf") {
            throw { code: "AlreadyQueued", message: "Ya está en la cola" };
          }
          return page([item("a/open")]);
        },
        (id) => navigated.push(id),
      );

      expect(await screen.findByRole("alert")).toHaveTextContent("Ya está en la cola");
      expect(navigated).toEqual([]);
      expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Explorar");
    });
  });

  it("explorar_enter_triggers_search", async () => {
    const calls = record(() => page([item("a/1")]));
    render(Explore);

    const box = screen.getByRole("searchbox");
    await fireEvent.input(box, { target: { value: "qwen" } });
    await fireEvent.keyDown(box, { key: "Enter" });

    await screen.findByText("a/1");
    expect(callsTo(calls, "hf_search")).toHaveLength(1);
    expect(callsTo(calls, "hf_search")[0].args).toEqual({ query: "qwen", sort: "downloads" });
  });
});
