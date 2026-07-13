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
</script>

<Modal {title} width={380} {onclose}>
  <p class="message">{message}</p>

  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button
      class={danger ? "danger" : "primary"}
      onclick={() => {
        // Confirm BEFORE closing: callers read their dialog state inside
        // onconfirm, and onclose clears it.
        onconfirm();
        onclose();
      }}
    >
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
