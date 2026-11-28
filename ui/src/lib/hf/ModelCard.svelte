<script lang="ts">
  import type { SearchItem } from "./types";
  import { fitHintBadge, fitHintTitle, formatCount, formatDate } from "./view-model";

  interface Props {
    item: SearchItem;
    onOpen?: (repoId: string) => void;
  }

  let { item, onOpen }: Props = $props();

  const model = $derived(item.model);
  const badge = $derived(fitHintBadge(item.hint));
</script>

<button type="button" class="model-card" onclick={() => onOpen?.(model.id)}>
  <strong>{model.id}</strong>
  <span>{formatCount(model.downloads)} descargas</span>
  <span>{formatCount(model.likes)} likes</span>
  {#if model.last_modified !== null}
    <span>{formatDate(model.last_modified)}</span>
  {/if}
  {#if model.gated !== "Open"}
    <span class="badge">Restringido</span>
  {/if}
  {#if badge !== null && item.hint !== null}
    <span class="badge" title={fitHintTitle(item.hint)}>{badge}</span>
  {/if}
</button>
