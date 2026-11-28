<script lang="ts">
  import { onMount } from "svelte";
  import { errorMessage } from "../errors";
  import * as api from "./api";
  import type { TokenStatus } from "./types";
  import { isValidTokenFormat, tokenSaveErrorMessage, tokenStatusLabel } from "./view-model";

  const FORMAT_HINT = "El token debe empezar por hf_ y tener al menos 30 caracteres alfanuméricos más";
  const NO_TOKEN: TokenStatus = { present: false, user: null, invalid: false };

  let status = $state<TokenStatus | null>(null);
  let token = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function exclusive(task: () => Promise<void>, describe: (failure: unknown) => string): Promise<void> {
    busy = true;
    error = null;
    try {
      await task();
    } catch (failure) {
      error = describe(failure);
    } finally {
      busy = false;
    }
  }

  async function submit(): Promise<void> {
    if (!isValidTokenFormat(token)) {
      error = FORMAT_HINT;
      return;
    }
    const candidate = token.trim();
    await exclusive(async () => {
      const identity = await api.hfTokenSave(candidate);
      status = { present: true, user: identity.name, invalid: false };
      token = "";
    }, tokenSaveErrorMessage);
  }

  function remove(): Promise<void> {
    return exclusive(async () => {
      await api.hfTokenDelete();
      status = NO_TOKEN;
    }, errorMessage);
  }

  onMount(() => {
    api.hfTokenStatus().then(
      (current) => (status = current),
      (failure) => (error = errorMessage(failure)),
    );
  });
</script>

<p>{tokenStatusLabel(status)}</p>

<label>
  Token de acceso
  <input type="password" bind:value={token} autocomplete="off" disabled={busy} />
</label>

<button type="button" onclick={submit} disabled={busy}>Guardar</button>
{#if status?.present}
  <button type="button" onclick={remove} disabled={busy}>Borrar</button>
{/if}

{#if error}
  <p role="alert">{error}</p>
{/if}
