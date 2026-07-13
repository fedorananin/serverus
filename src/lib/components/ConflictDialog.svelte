<script lang="ts">
  // Overwrite / Skip / Rename with "apply to all" (SPEC §6.1).
  import { transfers } from "$lib/stores/transfers.svelte";
  import Modal from "./Modal.svelte";

  const item = $derived(transfers.conflicted);
  let applyToAll = $state(false);

  function resolve(action: "overwrite" | "skip" | "rename") {
    if (!item) return;
    void transfers.resolve(item.session_id, item.id, action, applyToAll);
  }
</script>

{#if item}
  <Modal title="File already exists" width={420} onclose={() => resolve("skip")}>
    <p class="msg">
      <strong>{item.name}</strong> already exists at the destination.
    </p>
    <p class="target mono">
      {item.kind === "upload" ? item.remote_path : item.local_path}
    </p>
    <label class="all">
      <input type="checkbox" bind:checked={applyToAll} />
      <span>Apply to all remaining conflicts</span>
    </label>

    {#snippet footer()}
      <button onclick={() => resolve("skip")}>Skip</button>
      <button onclick={() => resolve("rename")}>Rename</button>
      <button class="danger" onclick={() => resolve("overwrite")}>Overwrite</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .msg {
    margin: 0 0 8px;
  }

  .target {
    color: var(--text-1);
    font-size: 11px;
    word-break: break-all;
    margin: 0 0 12px;
    user-select: text;
  }

  .all {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
</style>
