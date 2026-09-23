<script lang="ts">
  // One row of the transfer queue panel: direction, progress, per-item controls.
  import type { TransferSnapshot } from "$lib/app/contracts/api";
  import { useAppModel } from "$lib/app/model.svelte";
  import { formatCount, formatEta, formatSize, formatSpeed } from "$lib/format";

  interface Props {
    item: TransferSnapshot;
  }

  let { item }: Props = $props();

  const transfers = useAppModel().transfers;

  const ICONS = { upload: "↑", download: "↓", delete: "🗑", chmod: "🔑" } as const;
  /** Delete / chmod: `done`/`total` count entries, not bytes. */
  const treeOp = $derived(item.kind === "delete" || item.kind === "chmod");
  const verb = $derived(item.kind === "delete" ? "deleted" : "changed");
  // A server-side rm cannot be paused promptly — only cancelled.
  const pausable = $derived(!(item.kind === "delete" && item.accelerated));
  const amount = $derived(
    treeOp
      ? `${formatCount(item.done)} / ${formatCount(item.total)} items`
      : `${formatSize(item.done)}/${formatSize(item.total)}`,
  );
</script>

<div
  class="item"
  data-transfer-name={item.name}
  data-state={item.state}
  data-done={item.done}
  data-total={item.total}
>
  <span class="dir">{ICONS[item.kind]}</span>
  <span class="name" title={item.kind === "download" ? item.local_path : item.remote_path}>
    {item.name}
  </span>
  {#if item.accelerated && item.kind === "delete"}
    <span class="tar" title="Removed by the server's own rm — one command instead of per-entry requests">via rm</span>
  {:else if item.accelerated}
    <span class="tar" title="Streamed through remote tar — one stream instead of per-file round-trips">via tar</span>
  {/if}
  <span class="progress">
    {#if item.scanning}
      <span class="track"><span class="fill scanning"></span></span>
    {:else if item.state === "running" || item.state === "paused"}
      <span class="track">
        <span
          class="fill"
          style:width="{item.total > 0 ? Math.min(100, (item.done / item.total) * 100) : 0}%"
        ></span>
      </span>
    {/if}
  </span>
  <span class="meta mono">
    {#if item.state === "running" && item.scanning}
      scanning… {formatCount(item.total)} found
    {:else if item.state === "running"}
      {amount}
      {#if !treeOp}· {formatSpeed(item.speed_bps)}{/if}
      {#if item.speed_bps > 0}· {formatEta(item.total - item.done, item.speed_bps)}{/if}
    {:else if item.state === "done" && treeOp}
      <span class="ok">✓ {formatCount(item.total)} {item.total === 1 ? "item" : "items"} {verb}</span>
    {:else if item.state === "done"}
      <span class="ok">✓ {formatSize(item.total)}</span>
    {:else if item.state === "error"}
      <span class="bad" title={item.error}>{item.error}</span>
    {:else if item.state === "paused"}
      paused · {amount}
    {:else}
      {item.state}
    {/if}
  </span>
  <span class="controls">
    {#if item.state === "running" && pausable}
      <button onclick={() => transfers.pause(item.id)} title="Pause">⏸</button>
    {:else if item.state === "paused"}
      <button onclick={() => transfers.resume(item.id)} title="Resume">▶</button>
    {/if}
    {#if item.state === "running" || item.state === "paused" || item.state === "queued"}
      <button onclick={() => transfers.cancel(item.id)} title="Cancel">✕</button>
    {/if}
    {#if item.state === "error" || item.state === "cancelled"}
      <button
        onclick={() => transfers.retry(item.id)}
        title={treeOp ? "Retry (rescans what is left)" : "Retry (resumes partial files)"}>⟳</button
      >
    {/if}
  </span>
</div>

<style>
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

  /* Total still unknown: a sliding segment instead of a fill level. */
  .fill.scanning {
    width: 30%;
    animation: scan 1.2s ease-in-out infinite alternate;
  }

  @keyframes scan {
    from {
      transform: translateX(0);
    }
    to {
      transform: translateX(233%);
    }
  }

  .ok {
    color: var(--accent);
  }

  .bad {
    color: var(--danger);
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
</style>
