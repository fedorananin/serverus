<script lang="ts">
  // Collapsible transfer queue panel at the bottom of one tab's Files view
  // (SPEC §6.1). Per session: it shows only this tab's transfers.
  import { useAppModel } from "$lib/app/model.svelte";
  import TransferQueueItem from "./TransferQueueItem.svelte";

  interface Props {
    sessionId: string;
  }

  let { sessionId }: Props = $props();

  const transfers = useAppModel().transfers;
  const items = $derived(transfers.itemsFor(sessionId));
  const summary = $derived(transfers.summaryFor(sessionId));

  let collapsed = $state(true);
  // Auto-open when new work appears in this session (not on other tabs' work).
  let hadActive = false;
  $effect(() => {
    const active = summary.queued + summary.running > 0;
    if (active && !hadActive) collapsed = false;
    hadActive = active;
  });
</script>

{#if summary.total_items > 0}
  <div class="queue" class:collapsed>
    <button
      class="bar"
      data-testid="transfer-summary"
      data-total={summary.total_items}
      data-running={summary.running}
      data-queued={summary.queued}
      data-done={summary.done}
      data-failed={summary.failed}
      onclick={() => (collapsed = !collapsed)}
    >
      <span class="chevron">{collapsed ? "▴" : "▾"}</span>
      <span>Transfers</span>
      <span class="counts">
        {#if summary.running > 0}<span class="running">{summary.running} active</span>{/if}
        {#if summary.queued > 0}<span>{summary.queued} queued</span>{/if}
        {#if summary.done > 0}<span class="ok">{summary.done} done</span>{/if}
        {#if summary.failed > 0}<span class="bad">{summary.failed} failed</span>{/if}
      </span>
      <span class="bar-space"></span>
      {#if !collapsed}
        <span
          class="bar-btn"
          role="button"
          tabindex="-1"
          onclick={(e) => {
            e.stopPropagation();
            transfers.pauseAll(sessionId);
          }}
          onkeydown={() => {}}>⏸ all</span
        >
        <span
          class="bar-btn"
          role="button"
          tabindex="-1"
          onclick={(e) => {
            e.stopPropagation();
            transfers.resumeAll(sessionId);
          }}
          onkeydown={() => {}}>▶ all</span
        >
        <span
          class="bar-btn danger"
          role="button"
          tabindex="-1"
          onclick={(e) => {
            e.stopPropagation();
            transfers.cancelAll(sessionId);
          }}
          onkeydown={() => {}}>✕ all</span
        >
        <span
          class="bar-btn"
          role="button"
          tabindex="-1"
          onclick={(e) => {
            e.stopPropagation();
            void transfers.clearFinished(sessionId);
          }}
          onkeydown={() => {}}>clear</span
        >
      {/if}
    </button>

    {#if !collapsed}
      <div class="list">
        {#each items as item (item.id)}
          <TransferQueueItem {item} />
        {/each}
        {#if summary.total_items > items.length}
          <div class="more">…and {summary.total_items - items.length} more</div>
        {/if}
      </div>
    {/if}
  </div>
{/if}

<style>
  .queue {
    border-top: 1px solid var(--border);
    background: var(--bg-1);
    max-height: 220px;
    display: flex;
    flex-direction: column;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 12px;
    background: transparent;
    border: none;
    border-radius: 0;
    font-size: 11px;
    color: var(--text-1);
  }

  .chevron {
    font-size: 9px;
  }

  .counts {
    display: flex;
    gap: 10px;
  }

  .running {
    color: var(--accent);
  }

  .ok {
    color: var(--accent);
  }

  .bad {
    color: var(--danger);
  }

  .bar-space {
    flex: 1;
  }

  .bar-btn {
    padding: 1px 7px;
    border-radius: var(--radius);
    font-size: 10px;
  }

  .bar-btn:hover {
    background: var(--bg-3);
  }

  .bar-btn.danger:hover {
    color: var(--danger);
  }

  .list {
    overflow-y: auto;
    padding: 2px 8px 8px;
  }

  .more {
    font-size: 10px;
    color: var(--text-2);
    padding: 4px;
    text-align: center;
  }
</style>
