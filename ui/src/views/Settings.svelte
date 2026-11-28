<script lang="ts">
  import { onMount } from "svelte";
  import { getAppInfo } from "../lib/api";
  import { sectionLabel } from "../lib/navigation";
  import TokenSettings from "../lib/hf/TokenSettings.svelte";
  import RuntimeSettings from "../lib/runtime/RuntimeSettings.svelte";

  let versionText = $state("Versión …");

  onMount(() => {
    getAppInfo().then(
      (info) => (versionText = `Versión ${info.version}`),
      () => (versionText = "Versión no disponible"),
    );
  });
</script>

<h1>{sectionLabel("settings")}</h1>
<h2>llama.cpp</h2>
<RuntimeSettings />
<h2>Hugging Face</h2>
<TokenSettings />
<p>{versionText}</p>
