<script lang="ts">
  import { ExploreState } from "../lib/hf/explore-state.svelte";
  import ModelCard from "../lib/hf/ModelCard.svelte";
  import ModelDetail from "../lib/hf/ModelDetail.svelte";
  import type { SortBy } from "../lib/hf/types";
  import { sectionLabel } from "../lib/navigation";

  const explore = new ExploreState();
  let openRepoId = $state<string | null>(null);

  const SORT_OPTIONS: { value: SortBy; label: string }[] = [
    { value: "downloads", label: "Descargas" },
    { value: "likes", label: "Likes" },
  ];

  function onKeydown(event: KeyboardEvent): void {
    if (event.key === "Enter") void explore.search();
  }
</script>

<h1>{sectionLabel("explore")}</h1>

{#if openRepoId !== null}
  <button type="button" onclick={() => (openRepoId = null)}>Volver</button>
  <ModelDetail repoId={openRepoId} />
{:else}
  <input
    type="search"
    placeholder="Buscar modelos GGUF"
    aria-label="Buscar modelos"
    bind:value={explore.query}
    onkeydown={onKeydown}
  />
  <button type="button" onclick={() => explore.search()}>Buscar</button>

  <label>
    Ordenar por
    <select bind:value={explore.sort} onchange={() => explore.search()}>
      {#each SORT_OPTIONS as option (option.value)}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>
  </label>

  {#if explore.error}
    <p role="alert">{explore.error}</p>
    <button type="button" onclick={() => explore.search()}>Reintentar</button>
  {:else if explore.searched && explore.items.length === 0 && !explore.loading}
    <p>Sin resultados</p>
  {/if}

  <ul>
    {#each explore.items as item (item.model.id)}
      <li><ModelCard {item} onOpen={(repoId) => (openRepoId = repoId)} /></li>
    {/each}
  </ul>

  {#if explore.next !== null}
    <button type="button" onclick={() => explore.loadMore()} disabled={explore.loading}>Cargar más</button>
  {/if}
{/if}
