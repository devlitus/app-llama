<script lang="ts">
  import type { DownloadRequest, VariantView } from "./types";
  import { estimateLabel, fitLabel, formatGiB } from "./view-model";

  interface Props {
    view: VariantView;
    repoId: string;
    onDownload?: (request: DownloadRequest) => void;
  }

  const DEFAULT_REVISION = "main";

  let { view, repoId, onDownload }: Props = $props();

  const variant = $derived(view.variant);

  function download(): void {
    onDownload?.({ repo_id: repoId, revision: DEFAULT_REVISION, parts: variant.parts });
  }
</script>

<tr
  class:recommended={view.recommended}
  aria-label={view.recommended ? "Recomendada para este equipo" : undefined}
>
  <td>{variant.quant ?? "desconocida"}</td>
  <td>
    {formatGiB(variant.total_bytes)}
    {#if variant.parts.length > 1}
      <span>{variant.parts.length} partes</span>
    {/if}
    {#if !variant.complete}
      <span>incompleta</span>
    {/if}
  </td>
  <td>{estimateLabel(view.estimate)}</td>
  <td>
    {fitLabel(view.fit)}
    {#if view.recommended}
      <span class="badge">Recomendada</span>
    {/if}
  </td>
  <td>
    {#if variant.complete}
      <button type="button" disabled title="Disponible próximamente" onclick={download}>Descargar</button>
    {/if}
  </td>
</tr>
