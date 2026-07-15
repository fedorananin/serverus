<script lang="ts">
  import type { Settings, ThemePreference } from "$lib/api";
  import { commands, errorMessage, unwrap } from "$lib/api";
  import { vault } from "$lib/stores/vault.svelte";
  import { setThemePreference } from "$lib/theme";
  import { getVersion } from "@tauri-apps/api/app";
  import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
  import { onDestroy } from "svelte";
  import { isMac } from "$lib/platform";
  import Modal from "./Modal.svelte";
  import AboutSection from "./settings-dialog/AboutSection.svelte";
  import AppearanceSection from "./settings-dialog/AppearanceSection.svelte";
  import EditorSection from "./settings-dialog/EditorSection.svelte";
  import KnownHostsSection from "./settings-dialog/KnownHostsSection.svelte";
  import PanelsSection from "./settings-dialog/PanelsSection.svelte";
  import SecuritySection from "./settings-dialog/SecuritySection.svelte";
  import TerminalSection from "./settings-dialog/TerminalSection.svelte";
  import TransfersSection from "./settings-dialog/TransfersSection.svelte";
  import VaultSection from "./settings-dialog/VaultSection.svelte";
  import "./settings-dialog/settings-dialog.css";

  const REPO_URL = "https://github.com/fedorananin/serverus";

  interface Props {
    onclose: () => void;
  }

  let { onclose }: Props = $props();

  type SettingsDraft = Settings & { appearance: NonNullable<Settings["appearance"]> };

  function createSettingsDraft(source: Settings): SettingsDraft {
    const draft = structuredClone($state.snapshot(source)) as Settings;
    return {
      ...draft,
      appearance: draft.appearance ?? { theme: "system" },
    };
  }

  // Work on a deep copy; commit on Save.
  const initialSettings = createSettingsDraft(vault.data!.settings);
  let settings = $state<SettingsDraft>(initialSettings);
  let savedTheme = initialSettings.appearance.theme;
  let saving = $state(false);
  let error = $state<string | null>(null);
  let committed = false;
  let destroyed = false;

  onDestroy(() => {
    destroyed = true;
    if (!committed) setThemePreference(savedTheme);
  });

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
      if (destroyed || !vault.data) return;
      settings = createSettingsDraft(vault.data.settings);
      savedTheme = settings.appearance.theme;
      setThemePreference(savedTheme);
      importStatus = `Imported ${count} connection${count === 1 ? "" : "s"} ✓`;
    } catch (e) {
      importStatus = errorMessage(e);
    }
  }

  async function save() {
    if (saving) return;
    saving = true;
    error = null;
    const submitted = $state.snapshot(settings) as SettingsDraft;
    try {
      await vault.updateSettings(submitted);
      if (destroyed) return;
      committed = true;
      setThemePreference(submitted.appearance.theme);
      onclose();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      saving = false;
    }
  }

  function previewTheme(theme: ThemePreference) {
    settings.appearance.theme = theme;
    setThemePreference(theme);
  }

  function cancel() {
    if (saving) return;
    setThemePreference(savedTheme);
    onclose();
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

<Modal title="Settings" width={540} onclose={cancel}>
  <div class="form settings-dialog-form" inert={saving} aria-busy={saving}>
    <AppearanceSection value={settings.appearance.theme} onchange={previewTheme} />
    <SecuritySection
      bind:value={settings.security}
      biometryAvailable={vault.info?.biometry_available ?? false}
      quickUnlockMethod={vault.info?.quick_unlock_method ?? "biometrics"}
    />
    <TransfersSection bind:value={settings.transfers} />
    <EditorSection bind:value={settings.editor} />
    <TerminalSection bind:value={settings.terminal} {isMac} />
    <PanelsSection bind:value={settings.panels} />
    <VaultSection
      path={vault.info?.path}
      bind:newVaultPath
      {moveStatus}
      bind:currentPassword
      bind:newPassword
      bind:confirmPassword
      {canChangePassword}
      {passwordStatus}
      {exportStatus}
      {importStatus}
      onpickpath={() => void pickVaultPath()}
      onmove={() => void moveVault()}
      onchangepassword={() => void changePassword()}
      onexport={() => void exportConfig()}
      onimport={() => void importConfig()}
    />
    <KnownHostsSection
      {knownHosts}
      onforget={(hostKey) => void vault.removeKnownHost(hostKey)}
    />
    <AboutSection {appVersion} onopenrepo={() => void openRepo()} />

    {#if error}
      <div class="error">{error}</div>
    {/if}
  </div>

  {#snippet footer()}
    <button disabled={saving} onclick={cancel}>Cancel</button>
    <button class="primary" disabled={saving} onclick={() => void save()}>
      {saving ? "Saving…" : "Save"}
    </button>
  {/snippet}
</Modal>
