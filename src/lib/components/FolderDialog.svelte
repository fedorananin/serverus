<script lang="ts">
  import type { Badge } from "$lib/api";
  import { errorMessage } from "$lib/api";
  import { vault } from "$lib/stores/vault.svelte";
  import Modal from "./Modal.svelte";
  import BadgePicker from "./BadgePicker.svelte";

  interface Props {
    /** Folder being edited (id + current values), or null to create. */
    existing: { id: string; name: string; badge: Badge | null } | null;
    parentFolder: string | null;
    onclose: () => void;
  }

  let { existing, parentFolder, onclose }: Props = $props();

  let name = $state(existing?.name ?? "");
  let badge = $state<Badge | null>(existing?.badge ?? null);
  let saving = $state(false);
  let error = $state<string | null>(null);

  const canSave = $derived(name.trim() !== "" && !saving);

  async function save() {
    if (!canSave) return;
    saving = true;
    error = null;
    try {
      if (existing) {
        await vault.updateFolder(existing.id, name.trim(), badge);
      } else {
        await vault.createFolder(name.trim(), parentFolder, badge);
      }
      onclose();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      saving = false;
    }
  }

  function focusOnMount(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

<Modal title={existing ? "Edit folder" : "New folder"} width={380} {onclose}>
  <form
    class="form"
    onsubmit={(e) => {
      e.preventDefault();
      void save();
    }}
  >
    <label>
      <span>Name</span>
      <input type="text" bind:value={name} placeholder="Clients" use:focusOnMount />
    </label>
    <label>
      <span>Badge</span>
      <BadgePicker value={badge} onchange={(b) => (badge = b)} />
    </label>
    {#if error}
      <div class="error">{error}</div>
    {/if}
  </form>

  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button class="primary" disabled={!canSave} onclick={() => void save()}>
      {saving ? "Saving…" : existing ? "Save" : "Create"}
    </button>
  {/snippet}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  label > span {
    font-size: 11px;
    color: var(--text-1);
  }

  .error {
    color: var(--danger);
    font-size: 12px;
  }
</style>
