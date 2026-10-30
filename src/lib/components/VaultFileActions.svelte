<script lang="ts">
  import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
  import { vault } from "$lib/stores/vault.svelte";

  let vaultPath = $state("");

  /** Escape hatches for the lock screen: forgot the password, or the vault
   *  lives elsewhere — no unlock required to re-point the app. */
  async function openOtherVault() {
    const picked = await openFileDialog({
      multiple: false,
      directory: false,
      title: "Open vault",
      filters: [{ name: "Serverus vault", extensions: ["serverus"] }],
    });
    if (typeof picked === "string") await vault.switchVault(picked);
  }

  async function createNewVault() {
    const picked = await saveFileDialog({
      title: "Create new vault",
      defaultPath: "main.serverus",
      filters: [{ name: "Serverus vault", extensions: ["serverus"] }],
    });
    if (typeof picked === "string") await vault.switchVault(picked);
  }

  async function useVaultPath(event: SubmitEvent) {
    event.preventDefault();
    const path = vaultPath.trim();
    if (!path || vault.busy) return;
    await vault.switchVault(path);
    vaultPath = "";
  }
</script>

<!-- Lock-screen footer: the vault's path and the ways to point the app at
     another vault file. -->
<div class="vault-file">
  <div class="path mono" title={vault.info?.path}>{vault.info?.path}</div>
  {#if vault.info?.scenario_build}
    <!-- e2e-only escape hatch: WebDriver cannot drive the native pickers
         behind "Open another vault… / New vault…", so scenario builds keep
         a typed path field. Hidden in real builds — the pickers cover it. -->
    <form class="vault-path-form" onsubmit={useVaultPath}>
      <input
        class="vault-path-input mono"
        type="text"
        aria-label="Vault path"
        placeholder="/path/to/main.serverus"
        bind:value={vaultPath}
        disabled={vault.busy}
      />
      <button type="submit" class="subtle path-submit" disabled={vault.busy || !vaultPath.trim()}>
        Use path
      </button>
    </form>
  {/if}
  <div class="vault-actions">
    <button type="button" class="subtle" disabled={vault.busy} onclick={() => void openOtherVault()}>
      Open another vault…
    </button>
    <span class="sep">·</span>
    <button type="button" class="subtle" disabled={vault.busy} onclick={() => void createNewVault()}>
      New vault…
    </button>
  </div>
</div>

<style>
  /* The card's flex gap spaces these rows. */
  .vault-file {
    display: contents;
  }

  .path {
    margin-top: 16px;
    color: var(--text-2);
    font-size: 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .vault-path-form {
    display: flex;
    flex-direction: row;
    gap: 6px;
  }

  .vault-path-input {
    min-width: 0;
    flex: 1;
    text-align: left;
    font-size: 10px;
  }

  .path-submit {
    flex: 0 0 auto;
  }

  .vault-actions {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 6px;
  }

  .vault-actions .sep {
    color: var(--text-2);
    font-size: 10px;
  }

  .subtle {
    background: transparent;
    border-color: transparent;
    color: var(--text-1);
    font-size: 11px;
    padding: 2px 4px;
  }

  .subtle:hover {
    color: var(--accent);
  }
</style>
