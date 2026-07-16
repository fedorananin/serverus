<script lang="ts">
  import Modal from "./Modal.svelte";

  interface Props {
    onpaste: (text: string) => void;
    onfind: () => void;
  }

  let { onpaste, onfind }: Props = $props();
  let open = $state(false);
  let text = $state("");

  function close() {
    open = false;
    text = "";
  }

  function continuePaste() {
    const value = text;
    close();
    onpaste(value);
  }
</script>

<div class="toolbar">
  <button aria-label="Open terminal paste dialog" title="Paste…" onclick={() => (open = true)}>📋</button>
  <button aria-label="Open terminal find" title="Find…" onclick={onfind}>🔍</button>
</div>

{#if open}
  <Modal title="Paste into terminal" width={460} onclose={close}>
    <label>
      <span>Text</span>
      <textarea class="mono" aria-label="Terminal paste text" rows="6" bind:value={text}></textarea>
    </label>
    {#snippet footer()}
      <button onclick={close}>Cancel</button>
      <button class="primary" disabled={!text} onclick={continuePaste}>Continue</button>
    {/snippet}
  </Modal>
{/if}

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
  label { display: grid; gap: 6px; }
  textarea { width: 100%; resize: vertical; }
</style>
