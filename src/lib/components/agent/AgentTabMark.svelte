<script lang="ts">
  // A robot on a session tab while the AI agent is working in it.
  import { useAppModel } from "$lib/app/model.svelte";

  let { sessionId }: { sessionId: string | null } = $props();

  const agent = useAppModel().agent;
  const busy = $derived(agent.isBusy(sessionId));
</script>

{#if busy}
  <span class="mark" title="The AI agent is working in this tab" aria-label="AI agent working">🤖</span>
{/if}

<style>
  .mark {
    font-size: 11px;
    animation: pulse 1.6s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.45;
    }
  }
</style>
