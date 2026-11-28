import { errorMessage } from "../errors";
import { hfSearch, hfSearchNext } from "./api";
import type { SearchItem, SearchPage, SortBy } from "./types";

// HF pagina por un cursor sobre métricas que cambian: un modelo puede reaparecer
// en una página posterior y la clave del `{#each}` debe ser única.
function withoutRepeated(current: SearchItem[], incoming: SearchItem[]): SearchItem[] {
  return incoming.reduce(
    (merged, item) =>
      merged.some((known) => known.model.id === item.model.id) ? merged : [...merged, item],
    current,
  );
}

export class ExploreState {
  query = $state("");
  sort = $state<SortBy>("downloads");
  items = $state<SearchItem[]>([]);
  next = $state<string | null>(null);
  loading = $state(false);
  error = $state<string | null>(null);
  searched = $state(false);

  // Cada búsqueda nueva invalida las respuestas de las anteriores.
  private generation = 0;

  async search(): Promise<void> {
    const generation = ++this.generation;
    await this.fetch(generation, () => hfSearch(this.query, this.sort), (page) => {
      this.items = page.items;
      this.searched = true;
    });
  }

  async loadMore(): Promise<void> {
    const next = this.next;
    if (this.loading || next === null) return;
    await this.fetch(this.generation, () => hfSearchNext(next), (page) => {
      this.items = withoutRepeated(this.items, page.items);
    });
  }

  private async fetch(
    generation: number,
    request: () => Promise<SearchPage>,
    apply: (page: SearchPage) => void,
  ): Promise<void> {
    this.loading = true;
    this.error = null;
    try {
      const page = await request();
      if (generation !== this.generation) return;
      apply(page);
      this.next = page.next;
    } catch (failure) {
      if (generation === this.generation) this.error = errorMessage(failure);
    } finally {
      if (generation === this.generation) this.loading = false;
    }
  }
}
