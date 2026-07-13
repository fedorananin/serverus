<script lang="ts">
  import Modal from "./Modal.svelte";

  interface Props {
    /** Number of items about to be uploaded. */
    count: number;
    /** null = cancel the upload. */
    onchoice: (choice: "private" | "public_read" | null) => void;
  }

  let { count, onchoice }: Props = $props();
</script>

<Modal title="Upload access" width={380} onclose={() => onchoice(null)}>
  <p class="msg">
    Upload {count === 1 ? "1 item" : `${count} items`} as private or public?
  </p>

  {#snippet footer()}
    <button onclick={() => onchoice(null)}>Cancel</button>
    <button onclick={() => onchoice("private")}>Private</button>
    <button class="primary" onclick={() => onchoice("public_read")}>Public</button>
  {/snippet}
</Modal>

<style>
  .msg {
    margin: 0;
    font-size: 13px;
  }
</style>
