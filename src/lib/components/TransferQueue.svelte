<script lang="ts">
  // Collapsible transfer queue panel at the bottom of the window (SPEC §6.1).
  import { useAppModel } from "$lib/app/model.svelte";
  import { formatEta, formatSize, formatSpeed } from "$lib/format";

  const transfers = useAppModel().transfers;
  const items = $derived(transfers.items);
  const summary = $derived(transfers.summary);
</script>

{#if summary.total_items > 0}
  <div class="queue" class:collapsed={transfers.collapsed}>
    <button
      class="bar"
      data-testid="transfer-summary"
      data-total={summary.total_items}
      data-running={summary.running}
      data-queued={summary.queued}
      data-done={summary.done}
      data-failed={summary.failed}
      onclick={() => (transfers.collapsed = !transfers.collapsed)}
    >
      <span class="chevron">{transfers.collapsed ? "▴" : "▾"}</span>
      <span>Transfers</span>
      <span class="counts">
        {#if summary.running > 0}<span class="running">{summary.running} active</span>{/if}
        {#if summary.queued > 0}<span>{summary.queued} queued</span>{/if}
        {#if summary.done > 0}<span class="ok">{summary.done} done</span>{/if}
        {#if summary.failed > 0}<span class="bad">{summary.failed} failed</span>{/if}
      </span>
      <span class="bar-space"></span>
      {#if !transfers.collapsed}
        <span
          class="bar-btn"
          role="button"
          tabindex="-1"
          onclick={(e) => {
            e.stopPropagation();
            transfers.pauseAll();
          }}
          onkeydown={() => {}}>⏸ all</span
        >
        <span
          class="bar-btn"
          role="button"
          tabindex="-1"
          onclick={(e) => {
            e.stopPropagation();
            transfers.resumeAll();
          }}
          onkeydown={() => {}}>▶ all</span
        >
        <span
          class="bar-btn danger"
          role="button"
          tabindex="-1"
          onclick={(e) => {
            e.stopPropagation();
            transfers.cancelAll();
          }}
          onkeydown={() => {}}>✕ all</span
        >
        <span
          class="bar-btn"
          role="button"
          tabindex="-1"
          onclick={(e) => {
            e.stopPropagation();
            void transfers.clearFinished();
          }}
          onkeydown={() => {}}>clear</span
        >
      {/if}
    </button>

    {#if !transfers.collapsed}
      <div class="list">
        {#each items as item (item.id)}
          <div
            class="item"
            data-transfer-name={item.name}
            data-state={item.state}
            data-done={item.done}
            data-total={item.total}
          >
            <span class="dir">{item.kind === "upload" ? "↑" : "↓"}</span>
            <span class="name" title={item.kind === "upload" ? item.remote_path : item.local_path}>
              {item.name}
            </span>
            {#if item.accelerated}
              <span class="tar" title="Streamed through remote tar — one stream instead of per-file round-trips">via tar</span>
            {/if}
            <span class="progress">
              {#if item.state === "running" || item.state === "paused"}
                <span class="track">
                  <span
                    class="fill"
                    style:width="{item.total > 0 ? Math.min(100, (item.done / item.total) * 100) : 0}%"
                  ></span>
                </span>
              {/if}
            </span>
            <span class="meta mono">
              {#if item.state === "running"}
                {formatSize(item.done)}/{formatSize(item.total)}
                · {formatSpeed(item.speed_bps)}
                {#if item.speed_bps > 0}· {formatEta(item.total - item.done, item.speed_bps)}{/if}
              {:else if item.state === "done"}
                <span class="ok">✓ {formatSize(item.total)}</span>
              {:else if item.state === "error"}
                <span class="bad" title={item.error}>{item.error}</span>
              {:else if item.state === "paused"}
                paused · {formatSize(item.done)}/{formatSize(item.total)}
              {:else}
                {item.state}
              {/if}
            </span>
            <span class="controls">
              {#if item.state === "running"}
                <button onclick={() => transfers.pause(item.id)} title="Pause">⏸</button>
              {:else if item.state === "paused"}
                <button onclick={() => transfers.resume(item.id)} title="Resume">▶</button>
              {/if}
              {#if item.state === "running" || item.state === "paused" || item.state === "queued"}
                <button onclick={() => transfers.cancel(item.id)} title="Cancel">✕</button>
              {/if}
              {#if item.state === "error" || item.state === "cancelled"}
                <button onclick={() => transfers.retry(item.id)} title="Retry (resumes partial files)">⟳</button>
              {/if}
            </span>
          </div>
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

  .item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 4px;
    font-size: 11px;
  }

  .dir {
    color: var(--text-2);
    width: 12px;
  }

  .name {
    width: 180px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tar {
    font-size: 9px;
    color: var(--accent);
    border: 1px solid var(--accent-dim);
    border-radius: 3px;
    padding: 0 4px;
  }

  .progress {
    flex: 1;
    min-width: 60px;
  }

  .track {
    display: block;
    height: 4px;
    border-radius: 2px;
    background: var(--bg-3);
    overflow: hidden;
  }

  .fill {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.2s linear;
  }

  .meta {
    color: var(--text-2);
    font-size: 10px;
    max-width: 260px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .controls {
    display: flex;
    gap: 2px;
  }

  .controls button {
    padding: 0 5px;
    font-size: 10px;
    background: transparent;
    border-color: transparent;
    color: var(--text-1);
  }

  .controls button:hover {
    background: var(--bg-3);
  }

  .more {
    font-size: 10px;
    color: var(--text-2);
    padding: 4px;
    text-align: center;
  }
</style>
