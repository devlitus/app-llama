<script lang="ts">
  import { onMount } from "svelte";
  import { getAppInfo } from "../lib/api";
  import { sectionLabel } from "../lib/navigation";

  let versionText = $state("Versión …");

  onMount(() => {
    getAppInfo().then(
      (info) => (versionText = `Versión ${info.version}`),
      () => (versionText = "Versión no disponible"),
    );
  });
</script>

<h1>{sectionLabel("settings")}</h1>
<p>Próximamente</p>
<p>{versionText}</p>
