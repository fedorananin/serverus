<script lang="ts">
  import { vault } from "$lib/stores/vault.svelte";
  import { tabs } from "$lib/stores/tabs.svelte";
  import UnlockScreen from "./routes/UnlockScreen.svelte";
  import MainScreen from "./routes/MainScreen.svelte";

  $effect(() => {
    void vault.init();
  });

  // Keep the main screen (and its live terminals/sessions) mounted while
  // locked — SPEC §2.4: locking wipes keys, it does not kill sessions. The
  // opaque unlock overlay hides everything. Switching to another vault is
  // different: contextEpoch remounts the tree after vault-scoped state has
  // been discarded.
  const keepMainMounted = $derived(vault.screen === "main" || tabs.tabs.length > 0);
</script>

{#key vault.contextEpoch}
  {#if keepMainMounted}
    <MainScreen />
  {/if}

  {#if vault.screen === "unlock"}
    <div class="unlock-overlay">
      <UnlockScreen />
    </div>
  {/if}
{/key}
