<script lang="ts">
  import type { FilePaneNotice } from "./types";

  interface Props {
    total: number;
    selectedCount: number;
    notice: FilePaneNotice | null;
    /** The open folder lies inside a delete that is still running. */
    deletion?: string | null;
  }

  let { total, selectedCount, notice, deletion = null }: Props = $props();
</script>

<div class="statusbar">
  <span>{total} items{selectedCount > 0 ? `, ${selectedCount} selected` : ""}</span>
  {#if deletion}
    <span class="deletion" role="status" title={deletion}>This folder is being deleted…</span>
  {:else if notice}
    <span class="acl-note" class:err={notice.error} role={notice.error ? "alert" : "status"}
      >{notice.text}</span
    >
  {/if}
</div>

<style>
  .statusbar {
    padding: 3px 10px;
    border-top: 1px solid var(--border);
    font-size: 10px;
    color: var(--text-2);
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }

  .acl-note {
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .deletion {
    color: var(--warning);
  }

  .acl-note.err {
    color: var(--danger);
  }
</style>
