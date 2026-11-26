<script lang="ts">
  import type { Component } from "svelte";
  import { INITIAL_SECTION, SECTIONS, type SectionId } from "./lib/navigation";
  import Chat from "./views/Chat.svelte";
  import Explore from "./views/Explore.svelte";
  import Library from "./views/Library.svelte";
  import Run from "./views/Run.svelte";
  import Settings from "./views/Settings.svelte";

  const VIEWS: Record<SectionId, Component> = {
    explore: Explore,
    library: Library,
    run: Run,
    chat: Chat,
    settings: Settings,
  };

  let active = $state<SectionId>(INITIAL_SECTION);
  const ActiveView = $derived(VIEWS[active]);
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
    <ActiveView />
  </main>
</div>
