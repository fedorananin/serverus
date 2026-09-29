<script lang="ts">
  // Over a terminal the agent has worked in: what it is running, and the
  // controls to take the terminal over (the agent can no longer type here)
  // or hand it back.
  import { useAppModel } from "$lib/app/model.svelte";

  let { termId }: { termId: string | null } = $props();

  const agent = useAppModel().agent;
  const state = $derived(agent.terminal(termId));
</script>

{#if termId && state}
  <div class="banner" class:user={state.user_control} class:idle={!state.running && !state.user_control} role="status">
    {#if state.user_control}
      <span class="text">You have control — the AI agent cannot type here.</span>
      <button onclick={() => void agent.handBack(termId)}>Hand back to agent</button>
    {:else if state.running}
      <span class="text">
        🤖 Agent is running <code class="mono">{state.running}</code>
      </span>
      <button onclick={() => void agent.takeOver(termId)}>Take over</button>
    {:else}
      <span class="text">🤖 Shared with the AI agent</span>
      <button onclick={() => void agent.takeOver(termId)}>Take over</button>
    {/if}
  </div>
{/if}

<style>
  .banner {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    margin-left: 10px;
    padding: 1px 3px 1px 8px;
    background: var(--bg-2);
    border: 1px solid var(--accent);
    border-radius: var(--radius);
    font-size: 11px;
  }

  .banner.user {
    border-color: var(--border-strong);
  }

  /* Nothing running: stay out of the way until hovered. */
  .banner.idle {
    border-color: var(--border);
    opacity: 0.55;
  }

  .banner.idle:hover {
    opacity: 1;
  }

  .text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  code {
    font-size: 11px;
  }

  button {
    flex: none;
    font-size: 11px;
    padding: 1px 8px;
  }
</style>
