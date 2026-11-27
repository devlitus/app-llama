<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "./api";
  import type { BackendChoice, HardwareInfo, ProgressEvent, RuntimeStatus, UpdateInfo } from "./types";
  import {
    backendOptions,
    describeHardware,
    describeStatus,
    errorMessage,
    externalBinaryFilters,
    primaryAction,
    progressLabel,
    progressPercent,
  } from "./view-model";

  let hardware = $state<HardwareInfo | null>(null);
  let status = $state<RuntimeStatus | null>(null);
  let update = $state<UpdateInfo | null>(null);
  let selected = $state<BackendChoice>("auto");
  let progress = $state<ProgressEvent | null>(null);
  let busy = $state(false);
  let installing = $state(false);
  let error = $state<string | null>(null);

  const action = $derived(status ? primaryAction(status, update, selected) : null);
  const options = $derived(backendOptions(update));

  async function guarded(task: () => Promise<void>): Promise<void> {
    error = null;
    try {
      await task();
    } catch (failure) {
      error = errorMessage(failure);
    }
  }

  async function exclusive(task: () => Promise<void>): Promise<void> {
    busy = true;
    try {
      await guarded(task);
    } finally {
      busy = false;
    }
  }

  async function refreshStatus(): Promise<void> {
    status = await api.runtimeStatus();
    update = null;
    selected = "auto";
  }

  async function installWithProgress(): Promise<void> {
    const unlisten = await api.onRuntimeProgress((event) => (progress = event));
    try {
      await api.runtimeInstall(selected);
      await refreshStatus();
    } finally {
      unlisten();
    }
  }

  function install(): Promise<void> {
    return exclusive(async () => {
      installing = true;
      progress = null;
      try {
        await installWithProgress();
      } finally {
        installing = false;
        progress = null;
      }
    });
  }

  function runAction(): Promise<void> {
    if (action?.kind === "install") return install();
    if (action?.kind !== "check") return Promise.resolve();
    return exclusive(async () => {
      update = await api.runtimeCheckUpdate();
    });
  }

  function useExternal(): Promise<void> {
    return exclusive(async () => {
      const path = await api.pickExternalBinary(externalBinaryFilters(hardware?.os));
      if (path === null) return;
      await api.runtimeUseExternal(path);
      await refreshStatus();
    });
  }

  function cancel(): Promise<void> {
    return guarded(() => api.runtimeCancelInstall());
  }

  onMount(() => {
    void guarded(async () => {
      hardware = await api.runtimeHardware();
    });
    void guarded(refreshStatus);
  });
</script>

{#if hardware}
  <p>{describeHardware(hardware)}</p>
{/if}

{#if status}
  <p>{describeStatus(status)}</p>
{/if}

<label>
  Backend
  <select bind:value={selected} disabled={busy}>
    {#each options as option (option.value)}
      <option value={option.value}>{option.label}</option>
    {/each}
  </select>
</label>

{#if action}
  <button type="button" onclick={runAction} disabled={!action.enabled || busy}>
    {action.label}
  </button>
{/if}

<button type="button" onclick={useExternal} disabled={busy}>Usar binario existente…</button>

{#if installing}
  {#if progress}
    <p>{progressLabel(progress)}</p>
    <progress value={progressPercent(progress) ?? undefined} max="100"></progress>
  {/if}
  <button type="button" onclick={cancel}>Cancelar</button>
{/if}

{#if error}
  <p role="alert">{error}</p>
{/if}
