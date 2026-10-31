<script lang="ts">
  // Removing the master password strips the vault's confidentiality, so it
  // asks once more — and proves it is the user: the current password, or
  // Touch ID / Windows Hello when quick unlock is set up.
  import { commands, errorMessage, unwrap } from "$lib/api";
  import { vault } from "$lib/stores/vault.svelte";
  import Modal from "../Modal.svelte";

  interface Props {
    onclose: () => void;
    ondone: () => void;
  }

  let { onclose, ondone }: Props = $props();

  let password = $state("");
  let error = $state<string | null>(null);
  let busy = $state(false);

  const quick = $derived(vault.info?.quick_unlock_ready === true);

  function focus(node: HTMLInputElement) {
    node.focus();
  }

  async function remove(withQuickUnlock: boolean) {
    if (busy || (!withQuickUnlock && password === "")) return;
    busy = true;
    error = null;
    try {
      await unwrap(
        withQuickUnlock
          ? commands.vaultRemovePasswordQuick()
          : commands.vaultChangePassword(password, ""),
      );
      await vault.refreshInfo();
      ondone();
      onclose();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  function onsubmit(e: SubmitEvent) {
    e.preventDefault();
    void remove(false);
  }
</script>

<Modal title="Remove master password" width={400} {onclose}>
  <form class="remove" {onsubmit}>
    <p class="message">
      The vault will open without asking, and anyone who can read its file will be able to read
      your saved passwords and keys. Auto-lock and {vault.info?.quick_unlock_method ?? "Touch ID"}
      go away with it.
    </p>
    <input
      type="password"
      aria-label="Current master password"
      placeholder="Current master password"
      bind:value={password}
      disabled={busy}
      use:focus
    />
    {#if error}
      <div class="error" role="alert">{error}</div>
    {/if}
  </form>

  {#snippet footer()}
    <button type="button" onclick={onclose}>Cancel</button>
    {#if quick}
      <button type="button" disabled={busy} onclick={() => void remove(true)}>
        Use {vault.info?.quick_unlock_method}
      </button>
    {/if}
    <button
      type="button"
      class="danger"
      disabled={busy || password === ""}
      onclick={() => void remove(false)}
    >
      Remove password
    </button>
  {/snippet}
</Modal>

<style>
  .remove {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .message {
    margin: 0;
    color: var(--text-1);
  }

  .error {
    color: var(--danger);
    font-size: 12px;
  }
</style>
