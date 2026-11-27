<script lang="ts">
  import { onMount } from "svelte";
  import { getAppInfo } from "../lib/api";
  import { sectionLabel } from "../lib/navigation";
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
<p>{versionText}</p>
