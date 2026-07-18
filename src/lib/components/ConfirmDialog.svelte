<script lang="ts">
  import Modal from "./Modal.svelte";

  interface Props {
    title: string;
    message: string;
    confirmLabel?: string;
    danger?: boolean;
    onconfirm: () => void;
    onclose: () => void;
  }

  let { title, message, confirmLabel = "Delete", danger = true, onconfirm, onclose }: Props = $props();

  function confirm() {
    // Confirm BEFORE closing: callers read their dialog state inside
    // onconfirm, and onclose clears it.
    onconfirm();
    onclose();
  }

  // Pull focus off the file pane (whose Enter hotkey is rename) so keystrokes
  // land in the dialog while it is open.
  function focus(node: HTMLButtonElement) {
    node.focus();
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== "Enter") return;
    // A focused button (e.g. Cancel, reached via Tab) handles Enter natively.
    if (e.target instanceof HTMLButtonElement) return;
    e.preventDefault();
    confirm();
  }
</script>

<svelte:window {onkeydown} />

<Modal {title} width={380} {onclose}>
  <p class="message">{message}</p>

  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button class={danger ? "danger" : "primary"} use:focus onclick={confirm}>
      {confirmLabel}
    </button>
  {/snippet}
</Modal>

<style>
  .message {
    margin: 0;
    color: var(--text-1);
  }
</style>
