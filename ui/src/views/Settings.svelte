<script lang="ts">
  import { getAppInfo } from "../lib/api";
  import { sectionLabel } from "../lib/navigation";

  let versionText = $state("Versión …");

  $effect(() => {
    getAppInfo().then(
      (info) => (versionText = `Versión ${info.version}`),
      () => (versionText = "Versión no disponible"),
    );
  });
</script>

<h1>{sectionLabel("settings")}</h1>
<p>Próximamente</p>
<p>{versionText}</p>
