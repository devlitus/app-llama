import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";
import { ExploreState } from "./explore-state.svelte";
import type { SearchItem, SearchPage } from "./types";

afterEach(clearMocks);

function item(id: string): SearchItem {
  return {
    model: {
      id,
      downloads: 10,
      likes: 1,
      gated: "Open",
      last_modified: null,
      pipeline_tag: null,
      gguf: null,
    },
    hint: null,
  };
}

function page(ids: string[], next: string | null = null): SearchPage {
  return { items: ids.map(item), next };
}

describe("ExploreState", () => {
  it("explore_state_search_replaces_items", async () => {
    mockIPC(() => page(["new/model"]));
    const state = new ExploreState();
    state.items = [item("old/model")];

    await state.search();

    expect(state.items.map((i) => i.model.id)).toEqual(["new/model"]);
    expect(state.loading).toBe(false);
    expect(state.error).toBeNull();
  });

  it("explore_state_load_more_appends", async () => {
    mockIPC((cmd) =>
      cmd === "hf_search" ? page(["a/one"], "https://huggingface.co/next") : page(["b/two"]),
    );
    const state = new ExploreState();

    await state.search();
    await state.loadMore();

    expect(state.items.map((i) => i.model.id)).toEqual(["a/one", "b/two"]);
    expect(state.next).toBeNull();
  });

  it("explore_state_load_more_skips_duplicates", async () => {
    mockIPC((cmd) =>
      cmd === "hf_search"
        ? page(["a/1", "a/2"], "https://huggingface.co/next")
        : page(["a/2", "b/1", "b/1"], "https://huggingface.co/last"),
    );
    const state = new ExploreState();

    await state.search();
    await state.loadMore();

    expect(state.items.map((i) => i.model.id)).toEqual(["a/1", "a/2", "b/1"]);
    expect(state.next).toBe("https://huggingface.co/last");
  });

  it("explore_state_translates_error", async () => {
    mockIPC(() => {
      throw { code: "NotFound", message: "Modelo no encontrado" };
    });
    const state = new ExploreState();

    await state.search();

    expect(state.error).toBe("Modelo no encontrado");
    expect(state.loading).toBe(false);
  });

  it("explore_state_ignores_stale_response", async () => {
    const resolvers: Array<(value: SearchPage) => void> = [];
    mockIPC(
      () =>
        new Promise<SearchPage>((resolve) => {
          resolvers.push(resolve);
        }),
    );
    const state = new ExploreState();

    const first = state.search();
    const second = state.search();
    resolvers[1](page(["fresh/model"]));
    await second;
    resolvers[0](page(["stale/model"]));
    await first;

    expect(state.items.map((i) => i.model.id)).toEqual(["fresh/model"]);
  });
});
