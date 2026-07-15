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
  <button aria-label="Open terminal paste dialog" onclick={() => (open = true)}>Paste…</button>
  <button aria-label="Open terminal find" onclick={onfind}>Find…</button>
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
    position: absolute;
    top: 6px;
    left: 8px;
    z-index: 2;
    display: flex;
    gap: 4px;
  }
  .toolbar button {
    padding: 2px 7px;
    font-size: 11px;
    opacity: 0.72;
  }
  .toolbar button:hover { opacity: 1; }
  label { display: grid; gap: 6px; }
  textarea { width: 100%; resize: vertical; }
</style>
