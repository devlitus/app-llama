<script lang="ts">
  import { onMount } from "svelte";
  import { errorMessage } from "../errors";
  import { hfModelDetail } from "./api";
  import type { DownloadRequest, ModelDetail } from "./types";
  import VariantRow from "./VariantRow.svelte";
  import { budgetLegend } from "./view-model";

  interface Props {
    repoId: string;
    onDownload?: (request: DownloadRequest) => void;
  }

  let { repoId, onDownload }: Props = $props();

  let detail = $state<ModelDetail | null>(null);
  let error = $state<string | null>(null);

  const noneFits = $derived(
    detail !== null &&
      !detail.variants.some((view) => view.recommended) &&
      detail.variants.some((view) => view.fit === "NoFit"),
  );

  onMount(async () => {
    try {
      detail = await hfModelDetail(repoId);
    } catch (failure) {
      error = errorMessage(failure);
    }
  });
</script>

{#if error !== null}
  <p role="alert">{error}</p>
{:else if detail === null}
  <p>Cargando…</p>
{:else}
  <h2>{detail.info.id}</h2>
  <p>
    Licencia: {detail.info.license ?? "desconocida"}
    · Arquitectura: {detail.info.gguf?.architecture ?? "desconocida"}
    · Contexto máximo: {detail.info.gguf?.context_length ?? "desconocido"}
  </p>
  {#if detail.info.gated !== "Open"}
    <p>
      <span class="badge">Restringido</span>
      Requiere un token de Hugging Face (en Ajustes) y aceptar las condiciones en la web del modelo
    </p>
  {/if}
  {#if noneFits}
    <p>Ninguna cuantización cabe en la memoria de este equipo</p>
  {/if}
  <table>
    <thead>
      <tr>
        <th>Cuantización</th>
        <th>Tamaño</th>
        <th>Memoria estimada</th>
        <th>Encaje</th>
        <th></th>
      </tr>
    </thead>
    <tbody>
      {#each detail.variants as view (view.variant.parts[0].path)}
        <VariantRow {view} repoId={detail.info.id} {onDownload} />
      {/each}
    </tbody>
  </table>
  <p>RAM o VRAM aprox. con contexto de {detail.ctx} tokens</p>
  <p>{budgetLegend(detail.budget)}</p>
{/if}
