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
