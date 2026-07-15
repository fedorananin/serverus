<script lang="ts">
  interface Props {
    path?: string;
    newVaultPath: string;
    moveStatus: string | null;
    currentPassword: string;
    newPassword: string;
    confirmPassword: string;
    canChangePassword: boolean;
    passwordStatus: string | null;
    exportStatus: string | null;
    importStatus: string | null;
    onpickpath: () => void;
    onmove: () => void;
    onchangepassword: () => void;
    onexport: () => void;
    onimport: () => void;
  }

  let {
    path,
    newVaultPath = $bindable(),
    moveStatus,
    currentPassword = $bindable(),
    newPassword = $bindable(),
    confirmPassword = $bindable(),
    canChangePassword,
    passwordStatus,
    exportStatus,
    importStatus,
    onpickpath,
    onmove,
    onchangepassword,
    onexport,
    onimport,
  }: Props = $props();
</script>

<fieldset>
  <legend>Vault</legend>
  <div class="vault-path mono">{path}</div>
  <div class="move-vault">
    <input
      type="text"
      class="mono"
      placeholder="Folder or file path (e.g. ~/Dropbox or ~/Dropbox/main.serverus)"
      bind:value={newVaultPath}
    />
    <button type="button" onclick={onpickpath}>Choose…</button>
    <button type="button" disabled={!newVaultPath.trim()} onclick={onmove}>Move vault</button>
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
    <button type="button" disabled={!canChangePassword} onclick={onchangepassword}>
      Change master password
    </button>
  </div>
  {#if passwordStatus}
    <div class:error={!passwordStatus.endsWith("✓")} class:ok={passwordStatus.endsWith("✓")}>
      {passwordStatus}
    </div>
  {/if}
  <div class="export">
    <button type="button" onclick={onexport}>Export config (no secrets)…</button>
    <button type="button" onclick={onimport}>Import config…</button>
    <span class="hint">
      Export writes an UNENCRYPTED JSON copy — passwords and keys are omitted. Import merges a
      Serverus export or a hand-written file (see docs/CONFIG_FORMAT.md).
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
