<script lang="ts">
  // Always-visible reminder that every server is open to the AI agent
  // without confirmations.
  import { vault } from "$lib/stores/vault.svelte";

  let { onclick }: { onclick: () => void } = $props();

  const agent = $derived(vault.data?.settings.agent);
</script>

{#if agent?.enabled && agent.full_access}
  <button
    class="badge"
    title="AI agent has full access to every server, without confirmations. Click to change."
    {onclick}
  >
    🤖 Full access
  </button>
{/if}

<style>
  .badge {
    flex: none;
    margin-right: 4px;
    padding: 2px 8px;
    font-size: 11px;
    background: transparent;
    border: 1px solid var(--warning);
    color: var(--warning);
    border-radius: var(--radius);
  }
</style>
