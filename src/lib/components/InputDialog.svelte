<script lang="ts">
  import Modal from "./Modal.svelte";

  interface Props {
    title: string;
    placeholder?: string;
    initial?: string;
    confirmLabel?: string;
    onsubmit: (value: string) => void;
    onclose: () => void;
  }

  let { title, placeholder = "", initial = "", confirmLabel = "OK", onsubmit, onclose }: Props =
    $props();

  let value = $state(initial);

  function focusSelect(node: HTMLInputElement) {
    node.focus();
    // Select the stem only when renaming "name.ext".
    const dot = node.value.lastIndexOf(".");
    if (dot > 0) node.setSelectionRange(0, dot);
    else node.select();
  }

  function submit(e: Event) {
    e.preventDefault();
    const v = value.trim();
    if (!v) return;
    // Submit BEFORE closing: callers read their dialog state inside
    // onsubmit, and onclose clears it.
    onsubmit(v);
    onclose();
  }
</script>

<Modal {title} width={360} {onclose}>
  <form onsubmit={submit}>
    <input type="text" bind:value {placeholder} use:focusSelect class="wide" />
  </form>

  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button class="primary" disabled={!value.trim()} onclick={submit}>{confirmLabel}</button>
  {/snippet}
</Modal>

<style>
  .wide {
    width: 100%;
  }
</style>
