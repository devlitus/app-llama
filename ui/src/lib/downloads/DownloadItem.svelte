<script lang="ts">
  import { errorMessage } from "../errors";
  import {
    downloadsCancel,
    downloadsPause,
    downloadsResume,
    downloadsRetry,
  } from "./api";
  import { failureMessage, formatBytes, formatEta, formatSpeed } from "./format";
  import { actionsFor, type PhaseProgress, type UnitAction, type UnitView } from "./store.svelte";
  import type { DownloadId, FileSnapshot } from "./types";

  interface Props {
    unit: UnitView;
    onRemove: (id: DownloadId) => void;
  }

  let { unit, onRemove }: Props = $props();

  let actionError = $state<string | null>(null);

  const ACTION_LABELS: Record<UnitAction, string> = {
    pause: "Pausar",
    resume: "Reanudar",
    cancel: "Cancelar",
    retry: "Reintentar",
    remove: "Quitar",
  };

  const COMMANDS: Record<Exclude<UnitAction, "remove">, (id: DownloadId) => Promise<void>> = {
    pause: downloadsPause,
    resume: downloadsResume,
    cancel: downloadsCancel,
    retry: downloadsRetry,
  };

  function bytesOnDisk(files: readonly FileSnapshot[]): number {
    return files
      .filter((file) => file.status !== "Pending")
      .reduce((sum, file) => sum + file.size, 0);
  }

  // Sin evento de descarga, la barra sale del snapshot.
  const main = $derived(
    unit.progress?.download
      ? { value: unit.progress.download.downloaded_bytes, max: unit.progress.download.total_bytes }
      : { value: bytesOnDisk(unit.files), max: unit.total_bytes },
  );
  const verifying = $derived(unit.state === "Verifying" || unit.progress?.phase === "Verify");
  const verify = $derived(verifying ? unit.progress?.verify : undefined);
  // Las velocidades solo describen la fase del último evento.
  const rate = $derived<PhaseProgress | undefined>(
    verifying ? verify : unit.progress?.download,
  );
  const failure = $derived(typeof unit.state === "object" ? failureMessage(unit.state.Failed) : null);

  async function run(action: UnitAction): Promise<void> {
    actionError = null;
    if (action === "remove") {
      onRemove(unit.id);
      return;
    }
    try {
      await COMMANDS[action](unit.id);
    } catch (error) {
      actionError = errorMessage(error);
    }
  }
</script>

<article>
  <h2>{unit.display_name}</h2>
  <progress aria-label="Descarga" max={main.max} value={main.value}></progress>
  <span>{formatBytes(main.value)} de {formatBytes(main.max)}</span>
  {#if verifying}
    <span>Verificando</span>
    {#if verify}
      <progress aria-label="Verificación" max={verify.total_bytes} value={verify.downloaded_bytes}
      ></progress>
    {/if}
  {/if}
  {#if rate}
    <span>{verifying ? "Verificando… " : ""}{formatSpeed(rate.bytes_per_sec)}</span>
    <span>{formatEta(rate.eta_secs)}</span>
  {:else if verifying}
    <span>Verificando…</span>
  {/if}
  {#if failure !== null}
    <p>{failure}</p>
  {/if}
  {#if actionError !== null}
    <p role="alert">{actionError}</p>
  {/if}
  {#each actionsFor(unit.state) as action (action)}
    <button type="button" onclick={() => run(action)}>{ACTION_LABELS[action]}</button>
  {/each}
</article>
