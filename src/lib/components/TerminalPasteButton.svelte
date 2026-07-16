<script lang="ts">
  interface Props {
    /** Opens the terminal's paste dialog (shared with clipboard confirmation). */
    onpaste: () => void;
    onfind: () => void;
  }

  let { onpaste, onfind }: Props = $props();
</script>

<div class="toolbar">
  <button aria-label="Open terminal paste dialog" title="Paste…" onclick={onpaste}>📋</button>
  <button aria-label="Open terminal find" title="Find…" onclick={onfind}>🔍</button>
</div>

<style>
  .toolbar {
    display: flex;
    gap: 4px;
  }
  /* Same ghost look as the tab bar's ⚙/🔒, one step smaller and dimmed —
     secondary actions that shouldn't draw the eye. */
  .toolbar button {
    padding: 3px 6px;
    font-size: 11px;
    line-height: 1;
    background: transparent;
    border: none;
    color: var(--text-1);
    opacity: 0.55;
  }
  .toolbar button:hover {
    color: var(--text-0);
    opacity: 1;
  }
</style>
