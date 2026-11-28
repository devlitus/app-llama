<script lang="ts">
  import { onMount } from "svelte";
  import DownloadItem from "../lib/downloads/DownloadItem.svelte";
  import { DownloadsStore } from "../lib/downloads/store.svelte";
  import { errorMessage } from "../lib/errors";
  import { sectionLabel } from "../lib/navigation";

  const store = new DownloadsStore();
  let error = $state<string | null>(null);

  onMount(() => {
    store.start().catch((failure) => (error = errorMessage(failure)));
    return () => store.stop();
  });

  async function remove(id: number): Promise<void> {
    try {
      await store.remove(id);
    } catch (failure) {
      error = errorMessage(failure);
    }
  }
</script>

<h1>{sectionLabel("downloads")}</h1>

{#if error !== null}
  <p role="alert">{error}</p>
{/if}

{#if store.units.length === 0}
  <p>No hay descargas</p>
{:else}
  <ul>
    {#each store.units as unit (unit.id)}
      <li><DownloadItem {unit} onRemove={remove} /></li>
    {/each}
  </ul>
{/if}
