<script lang="ts">
  import type { Component } from "svelte";
  import { INITIAL_SECTION, SECTIONS, type SectionId } from "./lib/navigation";
  import Downloads from "./views/Downloads.svelte";
  import Chat from "./views/Chat.svelte";
  import Explore from "./views/Explore.svelte";
  import Library from "./views/Library.svelte";
  import Run from "./views/Run.svelte";
  import Settings from "./views/Settings.svelte";

  interface ViewProps {
    onNavigate?: (id: SectionId) => void;
  }

  // Las vistas sin props ignoran `onNavigate`; el cast evita declararlo en cada una.
  const VIEWS: Record<SectionId, Component<ViewProps> | Component<Record<string, never>>> = {
    explore: Explore,
    downloads: Downloads,
    library: Library,
    run: Run,
    chat: Chat,
    settings: Settings,
  };

  let active = $state<SectionId>(INITIAL_SECTION);
  const ActiveView = $derived(VIEWS[active] as Component<ViewProps>);
</script>

<div class="shell">
  <nav aria-label="Secciones">
    {#each SECTIONS as section (section.id)}
      <button
        type="button"
        aria-current={active === section.id ? "page" : undefined}
        onclick={() => (active = section.id)}
      >
        {section.label}
      </button>
    {/each}
  </nav>
  <main>
    <ActiveView onNavigate={(id: SectionId) => (active = id)} />
  </main>
</div>
