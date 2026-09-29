<script lang="ts">
  // The agent's journal for one tab: a toolbar button that appears once the
  // agent has done something here, and the list it opens.
  import { useAppModel } from "$lib/app/model.svelte";
  import type { AgentActivityEntry } from "$lib/app/contracts/api";

  let { sessionId }: { sessionId: string | null } = $props();

  const agent = useAppModel().agent;
  const entries = $derived(agent.activityFor(sessionId));
  const busy = $derived(agent.isBusy(sessionId));
  let open = $state(false);

  const icons: Record<AgentActivityEntry["status"], string> = {
    running: "⏳",
    done: "✓",
    failed: "✕",
    denied: "⊘",
    cancelled: "–",
  };

  function time(entry: AgentActivityEntry) {
    return new Date(entry.at_ms).toLocaleTimeString([], {
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  }
</script>

{#if entries.length > 0}
  <div class="wrap">
    <button
      class="toggle"
      class:busy
      aria-expanded={open}
      title="What the AI agent did in this tab"
      onclick={() => (open = !open)}
    >
      🤖 {busy ? "working…" : entries.length}
    </button>
    {#if open}
      <div class="panel" role="log" aria-label="AI agent activity">
        {#each entries as entry (entry.id)}
          <div class="entry" data-status={entry.status}>
            <span class="icon" aria-label={entry.status}>{icons[entry.status]}</span>
            <span class="when">{time(entry)}</span>
            <span class="tool mono">{entry.tool}</span>
            <span class="summary mono">{entry.summary}</span>
            {#if entry.detail}
              <span class="detail">{entry.detail}</span>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .wrap {
    position: relative;
  }

  .toggle {
    background: transparent;
    border: 1px solid var(--border);
    padding: 2px 8px;
    font-size: 11px;
    color: var(--text-1);
  }

  .toggle.busy {
    border-color: var(--accent);
    color: var(--text-0);
  }

  .panel {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 20;
    width: 460px;
    max-height: 360px;
    overflow-y: auto;
    background: var(--bg-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    box-shadow: var(--shadow-float);
    padding: 4px 0;
  }

  .entry {
    display: grid;
    grid-template-columns: 16px 62px 110px 1fr;
    gap: 6px;
    padding: 4px 10px;
    font-size: 11px;
    align-items: baseline;
  }

  .entry + .entry {
    border-top: 1px solid var(--border);
  }

  .icon {
    text-align: center;
  }

  .entry[data-status="done"] .icon {
    color: var(--accent);
  }

  .entry[data-status="failed"] .icon,
  .entry[data-status="denied"] .icon {
    color: var(--danger);
  }

  .when,
  .tool {
    color: var(--text-2);
  }

  .summary {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    user-select: text;
  }

  .detail {
    grid-column: 2 / -1;
    color: var(--text-1);
    white-space: pre-wrap;
    user-select: text;
  }
</style>
