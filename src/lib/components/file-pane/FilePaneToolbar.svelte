<script lang="ts">
  import type { S3UploadAcl } from "$lib/api";
  import type { PaneController, SortKey } from "$lib/stores/pane.svelte";

  interface Props {
    pane: PaneController;
    title: string;
    uploadMode?: S3UploadAcl | null;
    onuploadmode?: (mode: S3UploadAcl) => void;
    onactions: (event: MouseEvent) => void;
  }

  let { pane, title, uploadMode, onuploadmode, onactions }: Props = $props();
  let pathInput = $state("");
  let editingPath = $state(false);

  function startPathEdit() {
    pathInput = pane.path;
    editingPath = true;
  }

  function commitPath(event: Event) {
    event.preventDefault();
    editingPath = false;
    const target = pathInput.trim();
    if (target && target !== pane.path) void pane.navigate(target);
  }

  function sortIndicator(key: SortKey) {
    return pane.sortKey === key ? (pane.sortAsc ? " ↑" : " ↓") : "";
  }

  function autofocus(node: HTMLInputElement) {
    node.focus();
    node.select();
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

<div class="pathbar">
  {#if editingPath}
    <form onsubmit={commitPath} class="path-form">
      <input
        type="text"
        aria-label="{title} path input"
        class="mono path-input"
        bind:value={pathInput}
        onblur={() => (editingPath = false)}
        use:autofocus
      />
    </form>
  {:else}
    <button class="path mono" title={pane.path} aria-label="{title} path" onclick={startPathEdit}
      >{pane.path}</button
    >
  {/if}
  <input type="text" class="filter" placeholder="Filter" bind:value={pane.filter} />
</div>

<div class="cols">
  <button class="col name" onclick={() => pane.sortBy("name")}>Name{sortIndicator("name")}</button>
  <button class="col size" onclick={() => pane.sortBy("size")}>Size{sortIndicator("size")}</button>
  <button class="col date" onclick={() => pane.sortBy("mtime")}>Date{sortIndicator("mtime")}</button>
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

  .pathbar {
    display: flex;
    gap: 6px;
    padding: 5px 8px;
  }

  .path,
  .path-input {
    flex: 1;
    min-width: 0;
    text-align: left;
    font-size: 11px;
    padding: 3px 8px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
  }

  .path-form {
    flex: 1;
    display: flex;
  }

  .path-input {
    direction: ltr;
    width: 100%;
  }

  .filter {
    width: 84px;
    font-size: 11px;
    padding: 3px 7px;
  }

  .cols {
    display: flex;
    border-bottom: 1px solid var(--border);
    padding: 0 8px;
  }

  .col {
    background: transparent;
    border: none;
    color: var(--text-2);
    font-size: 10px;
    text-align: left;
    padding: 3px 4px;
  }

  .col.name {
    flex: 1;
  }

  .col.size {
    width: 66px;
  }

  .col.date {
    width: 78px;
  }

  .col.perm {
    width: 80px;
  }
</style>
