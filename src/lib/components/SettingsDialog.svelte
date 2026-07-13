<script lang="ts">
  import type { Settings } from "$lib/api";
  import { commands, errorMessage, unwrap } from "$lib/api";
  import { vault } from "$lib/stores/vault.svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
  import Modal from "./Modal.svelte";

  const REPO_URL = "https://github.com/fedorananin/serverus";

  interface Props {
    onclose: () => void;
  }

  let { onclose }: Props = $props();

  // Work on a deep copy; commit on Save.
  let settings = $state<Settings>(
    structuredClone($state.snapshot(vault.data!.settings)) as Settings,
  );
  let saving = $state(false);
  let error = $state<string | null>(null);

  // Change master password block.
  let currentPassword = $state("");
  let newPassword = $state("");
  let confirmPassword = $state("");
  let passwordStatus = $state<string | null>(null);
  let exportStatus = $state<string | null>(null);
  const canChangePassword = $derived(
    currentPassword !== "" && newPassword !== "" && newPassword === confirmPassword,
  );

  let newVaultPath = $state("");
  let moveStatus = $state<string | null>(null);

  async function moveVault() {
    moveStatus = null;
    try {
      let path = newVaultPath.trim();
      if (path.startsWith("~/")) {
        const home = await unwrap(commands.localHome());
        path = home + path.slice(1);
      }
      await unwrap(commands.vaultSetPath(path));
      await vault.refreshInfo();
      newVaultPath = "";
      moveStatus = "Vault moved ✓";
    } catch (e) {
      moveStatus = errorMessage(e);
    }
  }

  /** Native save panel: pick the new vault location visually; the text
   *  field stays for pasting a path by hand. */
  async function pickVaultPath() {
    const current = vault.info?.path ?? "";
    const picked = await saveFileDialog({
      title: "Move vault to…",
      defaultPath: current.split("/").pop() || "main.serverus",
      filters: [{ name: "Serverus vault", extensions: ["serverus"] }],
    });
    if (typeof picked === "string") newVaultPath = picked;
  }

  async function exportConfig() {
    exportStatus = null;
    try {
      const home = await unwrap(commands.localHome());
      const path = `${home}/serverus-config-export.json`;
      await unwrap(commands.vaultExportConfig(path));
      exportStatus = `Saved to ${path} ✓`;
    } catch (e) {
      exportStatus = errorMessage(e);
    }
  }

  let importStatus = $state<string | null>(null);

  async function importConfig() {
    importStatus = null;
    const picked = await openFileDialog({
      multiple: false,
      directory: false,
      title: "Import Serverus config",
      filters: [{ name: "JSON config", extensions: ["json"] }],
    });
    if (typeof picked !== "string") return;
    try {
      const count = await vault.importConfig(picked);
      importStatus = `Imported ${count} connection${count === 1 ? "" : "s"} ✓`;
    } catch (e) {
      importStatus = errorMessage(e);
    }
  }

  async function save() {
    saving = true;
    error = null;
    try {
      await vault.updateSettings($state.snapshot(settings) as Settings);
      onclose();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      saving = false;
    }
  }

  async function changePassword() {
    passwordStatus = null;
    try {
      await unwrap(commands.vaultChangePassword(currentPassword, newPassword));
      passwordStatus = "Master password changed ✓";
      currentPassword = newPassword = confirmPassword = "";
    } catch (e) {
      passwordStatus = errorMessage(e);
    }
  }

  const knownHosts = $derived(Object.entries(vault.data?.known_hosts ?? {}));

  let appVersion = $state("");
  void getVersion().then((v) => (appVersion = v));

  async function openRepo() {
    // Best-effort: opening the browser must never surface an error here.
    try {
      await unwrap(commands.openExternal(REPO_URL));
    } catch {
      /* ignore */
    }
  }
</script>

<Modal title="Settings" width={540} {onclose}>
  <div class="form">
    <fieldset>
      <legend>Security</legend>
      <div class="row">
        <label>
          <span>Auto-lock after (minutes, 0 = never)</span>
          <input type="number" min="0" max="1440" bind:value={settings.security.auto_lock_minutes} />
        </label>
        <label class="checkbox">
          <input type="checkbox" bind:checked={settings.security.lock_on_sleep} />
          <span>Lock when Mac sleeps</span>
        </label>
      </div>
      <label class="checkbox">
        <input
          type="checkbox"
          bind:checked={settings.security.touch_id}
          disabled={!vault.info?.biometry_available}
        />
        <span>Unlock with Touch ID{vault.info?.biometry_available ? "" : " (not available)"}</span>
      </label>
    </fieldset>

    <fieldset>
      <legend>Transfers</legend>
      <div class="row">
        <label>
          <span>Parallel files per server</span>
          <input type="number" min="1" max="16" bind:value={settings.transfers.max_parallel_per_server} />
        </label>
        <label>
          <span>On conflict</span>
          <select bind:value={settings.transfers.conflict_policy}>
            <option value="ask">Ask</option>
            <option value="overwrite">Overwrite</option>
            <option value="skip">Skip</option>
            <option value="rename">Rename</option>
          </select>
        </label>
      </div>
      <label class="checkbox">
        <input type="checkbox" bind:checked={settings.transfers.preserve_mtime} />
        <span>Preserve modification times</span>
      </label>
      <label class="checkbox">
        <input type="checkbox" bind:checked={settings.transfers.tar_acceleration} />
        <span>Accelerate folder transfers via tar stream when available</span>
      </label>
    </fieldset>

    <fieldset>
      <legend>Editor</legend>
      <label class="checkbox">
        <input type="checkbox" bind:checked={settings.editor.use_system_default} />
        <span>Open remote files with the system default app</span>
      </label>
      {#if !settings.editor.use_system_default}
        <label>
          <span>Application</span>
          <input type="text" bind:value={settings.editor.custom_app} placeholder="Visual Studio Code" />
        </label>
      {/if}
    </fieldset>

    <fieldset>
      <legend>Terminal</legend>
      <div class="row">
        <label>
          <span>Font</span>
          <input type="text" bind:value={settings.terminal.font_family} />
        </label>
        <label class="narrow">
          <span>Size</span>
          <input type="number" min="8" max="32" bind:value={settings.terminal.font_size} />
        </label>
        <label class="narrow">
          <span>Scrollback</span>
          <input type="number" min="100" max="100000" step="100" bind:value={settings.terminal.scrollback} />
        </label>
      </div>
      <label class="checkbox">
        <input type="checkbox" bind:checked={settings.terminal.copy_on_select} />
        <span>Copy selection to clipboard automatically (otherwise use ⌘C)</span>
      </label>
    </fieldset>

    <fieldset>
      <legend>Panels</legend>
      <div class="row">
        <label class="checkbox">
          <input type="checkbox" bind:checked={settings.panels.show_hidden} />
          <span>Show hidden files</span>
        </label>
        <label>
          <span>Size format</span>
          <select bind:value={settings.panels.size_format}>
            <option value="kib">KiB (1024)</option>
            <option value="kb">KB (1000)</option>
          </select>
        </label>
      </div>
      <label>
        <span>Default local folder</span>
        <input type="text" bind:value={settings.panels.default_local_dir} placeholder="~" class="mono" />
      </label>
    </fieldset>

    <fieldset>
      <legend>Vault</legend>
      <div class="vault-path mono">{vault.info?.path}</div>
      <div class="move-vault">
        <input
          type="text"
          class="mono"
          placeholder="New vault path (e.g. ~/Dropbox/main.serverus)"
          bind:value={newVaultPath}
        />
        <button type="button" onclick={() => void pickVaultPath()}>Choose…</button>
        <button type="button" disabled={!newVaultPath.trim()} onclick={() => void moveVault()}>
          Move vault
        </button>
      </div>
      {#if moveStatus}
        <div class:error={!moveStatus.endsWith("✓")} class:ok={moveStatus.endsWith("✓")}>
          {moveStatus}
        </div>
      {/if}
      <div class="pw-change">
        <input type="password" placeholder="Current password" bind:value={currentPassword} />
        <input type="password" placeholder="New password" bind:value={newPassword} />
        <input type="password" placeholder="Repeat new" bind:value={confirmPassword} />
        <button type="button" disabled={!canChangePassword} onclick={() => void changePassword()}>
          Change master password
        </button>
      </div>
      {#if passwordStatus}
        <div class:error={!passwordStatus.endsWith("✓")} class:ok={passwordStatus.endsWith("✓")}>
          {passwordStatus}
        </div>
      {/if}
      <div class="export">
        <button type="button" onclick={() => void exportConfig()}>
          Export config (no secrets)…
        </button>
        <button type="button" onclick={() => void importConfig()}>Import config…</button>
        <span class="hint">
          Export writes an UNENCRYPTED JSON copy — passwords and keys are omitted. Import merges
          a Serverus export or a hand-written file (see docs/CONFIG_FORMAT.md).
        </span>
      </div>
      {#if exportStatus}
        <div class:error={!exportStatus.endsWith("✓")} class:ok={exportStatus.endsWith("✓")}>
          {exportStatus}
        </div>
      {/if}
      {#if importStatus}
        <div class:error={!importStatus.endsWith("✓")} class:ok={importStatus.endsWith("✓")}>
          {importStatus}
        </div>
      {/if}
    </fieldset>

    {#if knownHosts.length > 0}
      <fieldset>
        <legend>Known hosts</legend>
        {#each knownHosts as [hostKey] (hostKey)}
          <div class="known-host">
            <span class="mono">{hostKey}</span>
            <button type="button" class="small" onclick={() => void vault.removeKnownHost(hostKey)}>
              Forget
            </button>
          </div>
        {/each}
      </fieldset>
    {/if}

    <fieldset>
      <legend>About</legend>
      <div class="about">
        <span class="app-name">Serverus{appVersion ? ` v${appVersion}` : ""}</span>
        <button type="button" class="repo-link" onclick={() => void openRepo()}>
          github.com/fedorananin/serverus
        </button>
      </div>
      <div class="about-note">Written entirely by AI · MIT-licensed · macOS</div>
    </fieldset>

    {#if error}
      <div class="error">{error}</div>
    {/if}
  </div>

  {#snippet footer()}
    <button onclick={onclose}>Cancel</button>
    <button class="primary" disabled={saving} onclick={() => void save()}>
      {saving ? "Saving…" : "Save"}
    </button>
  {/snippet}
</Modal>

<style>
  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  fieldset {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px 12px 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin: 0;
  }

  legend {
    font-size: 11px;
    color: var(--text-1);
    padding: 0 4px;
  }

  .row {
    display: flex;
    gap: 14px;
    align-items: flex-end;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }

  label > span {
    font-size: 11px;
    color: var(--text-1);
  }

  label.narrow {
    max-width: 90px;
  }

  .checkbox {
    flex-direction: row;
    align-items: center;
    gap: 6px;
  }

  .checkbox > span {
    font-size: 13px;
    color: var(--text-0);
  }

  .vault-path {
    font-size: 11px;
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pw-change {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .move-vault {
    display: flex;
    gap: 8px;
  }

  .move-vault input {
    flex: 1;
    font-size: 11px;
  }

  .export {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .export .hint {
    font-size: 10px;
    color: var(--text-2);
  }

  .known-host {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 11px;
  }

  .small {
    font-size: 11px;
    padding: 2px 8px;
  }

  .error {
    color: var(--danger);
    font-size: 12px;
  }

  .ok {
    color: var(--accent);
    font-size: 12px;
  }

  .about {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 10px;
  }

  .about .app-name {
    font-size: 12px;
    color: var(--text-0);
  }

  .repo-link {
    background: none;
    border: none;
    padding: 0;
    font-size: 12px;
    color: var(--accent);
    cursor: pointer;
  }

  .repo-link:hover {
    text-decoration: underline;
  }

  .about-note {
    font-size: 10px;
    color: var(--text-2);
  }
</style>
