<script lang="ts">
  import type { S3UploadAcl } from "$lib/api";
  import type { PaneController, SortKey } from "$lib/stores/pane.svelte";
  import type { FileColumn, FileColumnWidths } from "./column-widths.svelte";
  import FilePanePathBar from "./FilePanePathBar.svelte";

  interface Props {
    pane: PaneController;
    title: string;
    columns: FileColumnWidths;
    uploadMode?: S3UploadAcl | null;
    onuploadmode?: (mode: S3UploadAcl) => void;
    onactions: (event: MouseEvent) => void;
  }

  let { pane, title, columns, uploadMode, onuploadmode, onactions }: Props = $props();

  function sortIndicator(key: SortKey) {
    return pane.sortKey === key ? (pane.sortAsc ? " ↑" : " ↓") : "";
  }
</script>

<div class="pane-head">
  <span class="pane-title">{title}</span>
  {#if uploadMode && onuploadmode}
    <select
      class="acl-mode"
      title="Access for uploaded files"
      value={uploadMode}
      onchange={(event) => onuploadmode(event.currentTarget.value as S3UploadAcl)}
    >
      <option value="private">upload: private</option>
      <option value="public_read">upload: public</option>
      <option value="ask">upload: ask</option>
    </select>
  {/if}
  <button class="tool" title="Up" aria-label="Up" onclick={() => void pane.up()}>↑</button>
  <button class="tool" title="Refresh" aria-label="Refresh" onclick={() => void pane.refresh()}
    >⟳</button
  >
  <button
    class="tool"
    title="Actions"
    aria-label="{pane.side === 'local' ? 'Local' : 'Remote'} pane actions"
    onclick={onactions}>⋯</button
  >
  <button
    class="tool"
    title="Toggle hidden files"
    aria-label="Toggle hidden files"
    class:on={pane.showHidden}
    onclick={() => (pane.showHidden = !pane.showHidden)}>.*</button
  >
</div>

<FilePanePathBar {pane} {title} />

{#snippet resizer(column: FileColumn, label: string)}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <span
    class="col-resize"
    title="Drag to resize · double-click to reset"
    aria-label="Resize {label} column"
    onpointerdown={(event) => columns.startResize(column, event)}
    ondblclick={() => columns.reset(column)}
  ></span>
{/snippet}

<div class="cols">
  <button class="col name" onclick={() => pane.sortBy("name")}>Name{sortIndicator("name")}</button>
  {@render resizer("size", "Size")}
  <button class="col size" onclick={() => pane.sortBy("size")}>Size{sortIndicator("size")}</button>
  {@render resizer("date", "Date")}
  <button class="col date" onclick={() => pane.sortBy("mtime")}>Date{sortIndicator("mtime")}</button>
  {@render resizer("mode", pane.s3 ? "Access" : "Mode")}
  <button class="col perm" onclick={() => pane.sortBy("permissions")}>
    {pane.s3 ? "Access" : "Mode"}{sortIndicator("permissions")}
  </button>
</div>

<style>
  .pane-head {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 5px 8px 0;
  }

  .pane-title {
    flex: 1;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-1);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .tool {
    padding: 1px 7px;
    font-size: 12px;
    background: transparent;
    border-color: transparent;
    color: var(--text-1);
  }

  .tool:hover {
    background: var(--bg-3);
  }

  .tool.on {
    color: var(--accent);
  }

  .acl-mode {
    font-size: 10px;
    padding: 1px 4px;
  }

  .cols {
    display: flex;
    border-bottom: 1px solid var(--border);
    padding: 0 8px;
    overflow: hidden;
  }

  .col {
    background: transparent;
    border: none;
    color: var(--text-2);
    font-size: 10px;
    text-align: left;
    padding: 3px 4px;
  }

  /* Name never shrinks away: in a pane too narrow for the widened columns,
     those are clipped at the right edge instead (see FilePaneRow). */
  .col.name {
    flex: 1;
    min-width: 64px;
  }

  .col.size {
    width: var(--col-size);
    padding-right: 0;
    text-align: right;
  }

  .col.date {
    width: var(--col-date);
    padding-left: 8px;
  }

  .col.perm {
    width: var(--col-mode);
    padding-left: 8px;
  }

  .col.size,
  .col.date,
  .col.perm {
    flex-shrink: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Zero-width in the flow, so header and row columns stay aligned; the
     grab area straddles the boundary. */
  .col-resize {
    position: relative;
    flex: 0 0 0;
    align-self: stretch;
  }

  .col-resize::before {
    content: "";
    position: absolute;
    top: 3px;
    bottom: 3px;
    left: -4px;
    width: 8px;
    cursor: col-resize;
    background: linear-gradient(var(--border), var(--border)) center / 1px 100% no-repeat;
    z-index: 1;
  }

  .col-resize:hover::before {
    background-image: linear-gradient(var(--accent), var(--accent));
  }
</style>
