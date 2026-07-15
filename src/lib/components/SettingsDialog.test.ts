// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { waitFor } from "@testing-library/svelte";

import type { Settings } from "$lib/api";
import SettingsDialog from "./SettingsDialog.svelte";

const mocks = vi.hoisted(() => ({
  updateSettings: vi.fn(),
  importConfig: vi.fn(),
  openFileDialog: vi.fn(),
  setThemePreference: vi.fn(),
  settings: null as Settings | null,
}));

vi.mock("$lib/theme", () => ({
  setThemePreference: mocks.setThemePreference,
}));

vi.mock("$lib/api", () => ({
  commands: {
    localHome: vi.fn(),
    openExternal: vi.fn(),
    vaultChangePassword: vi.fn(),
    vaultExportConfig: vi.fn(),
    vaultSetPath: vi.fn(),
  },
  errorMessage: (error: unknown) => (error instanceof Error ? error.message : String(error)),
  unwrap: async (promise: Promise<{ status: "ok"; data: unknown }>) => (await promise).data,
}));

vi.mock("$lib/stores/vault.svelte", () => ({
  vault: {
    get data() {
      return { settings: mocks.settings, known_hosts: {} };
    },
    info: {
      path: "/vaults/main.serverus",
      biometry_available: true,
      quick_unlock_method: "Touch ID",
    },
    updateSettings: mocks.updateSettings,
    refreshInfo: vi.fn(),
    importConfig: mocks.importConfig,
    removeKnownHost: vi.fn(),
  },
}));

vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(async () => "1.1.2") }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: mocks.openFileDialog,
  save: vi.fn(async () => null),
}));

function initialSettings(): Settings {
  return {
    appearance: { theme: "system" },
    security: { auto_lock_minutes: 5, lock_on_sleep: true, touch_id: true },
    transfers: {
      max_parallel_per_server: 2,
      conflict_policy: "ask",
      preserve_mtime: true,
      tar_acceleration: true,
    },
    editor: { use_system_default: true, custom_app: null },
    terminal: {
      font_family: "monospace",
      font_size: 13,
      scrollback: 10_000,
      copy_on_select: false,
    },
    panels: { show_hidden: false, size_format: "kib", default_local_dir: null },
  };
}

describe("SettingsDialog transaction", () => {
  beforeEach(() => {
    mocks.settings = initialSettings();
    mocks.updateSettings.mockReset();
    mocks.updateSettings.mockResolvedValue(undefined);
    mocks.importConfig.mockReset();
    mocks.openFileDialog.mockReset();
    mocks.openFileDialog.mockResolvedValue(null);
    mocks.setThemePreference.mockReset();
  });

  it("keeps edits isolated until Save and submits the complete settings copy", async () => {
    const onclose = vi.fn();
    render(SettingsDialog, { onclose });

    await fireEvent.input(
      screen.getByLabelText("Auto-lock after (minutes, 0 = never)"),
      { target: { value: "30" } },
    );
    await fireEvent.change(screen.getByLabelText("On conflict"), {
      target: { value: "rename" },
    });
    await fireEvent.click(
      screen.getByLabelText("Open remote files with the system default app"),
    );
    await fireEvent.input(screen.getByLabelText("Application"), {
      target: { value: "Visual Studio Code" },
    });
    await fireEvent.input(screen.getByLabelText("Font"), {
      target: { value: "JetBrains Mono" },
    });
    await fireEvent.click(screen.getByLabelText("Show hidden files"));

    expect(mocks.settings).toEqual(initialSettings());
    expect(mocks.updateSettings).not.toHaveBeenCalled();

    await fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(mocks.updateSettings).toHaveBeenCalledWith({
      ...initialSettings(),
      security: { ...initialSettings().security, auto_lock_minutes: 30 },
      transfers: { ...initialSettings().transfers, conflict_policy: "rename" },
      editor: { use_system_default: false, custom_app: "Visual Studio Code" },
      terminal: { ...initialSettings().terminal, font_family: "JetBrains Mono" },
      panels: { ...initialSettings().panels, show_hidden: true },
    });
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("discards the working copy on Cancel", async () => {
    const onclose = vi.fn();
    render(SettingsDialog, { onclose });
    await fireEvent.input(
      screen.getByLabelText("Auto-lock after (minutes, 0 = never)"),
      { target: { value: "60" } },
    );

    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(mocks.updateSettings).not.toHaveBeenCalled();
    expect(mocks.settings?.security.auto_lock_minutes).toBe(5);
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("previews theme radio changes immediately and restores the saved theme on Cancel", async () => {
    const onclose = vi.fn();
    render(SettingsDialog, { onclose });

    await fireEvent.click(screen.getByRole("radio", { name: "Light" }));

    expect(mocks.setThemePreference).toHaveBeenLastCalledWith("light");
    expect(screen.getByRole("radio", { name: "Light" })).toBeChecked();

    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(mocks.setThemePreference).toHaveBeenLastCalledWith("system");
    expect(mocks.updateSettings).not.toHaveBeenCalled();
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("persists the selected theme with the rest of the settings", async () => {
    const onclose = vi.fn();
    render(SettingsDialog, { onclose });

    await fireEvent.click(screen.getByRole("radio", { name: "Dark" }));
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect(mocks.updateSettings).toHaveBeenCalledWith({
      ...initialSettings(),
      appearance: { theme: "dark" },
    });
    expect(mocks.setThemePreference).toHaveBeenLastCalledWith("dark");
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("restores the persisted theme when the dialog is torn down without closing", async () => {
    const view = render(SettingsDialog, { onclose: vi.fn() });

    await fireEvent.click(screen.getByRole("radio", { name: "Light" }));
    view.unmount();

    expect(mocks.setThemePreference).toHaveBeenLastCalledWith("system");
  });

  it("uses imported settings as the new draft and rollback baseline", async () => {
    const onclose = vi.fn();
    const importedSettings = initialSettings();
    importedSettings.appearance = { theme: "dark" };
    importedSettings.security.auto_lock_minutes = 42;
    mocks.openFileDialog.mockResolvedValue("/tmp/serverus-config.json");
    mocks.importConfig.mockImplementation(async () => {
      mocks.settings = importedSettings;
      return 2;
    });
    render(SettingsDialog, { onclose });

    await fireEvent.click(screen.getByRole("radio", { name: "Light" }));
    await fireEvent.click(screen.getByRole("button", { name: "Import config…" }));

    await waitFor(() => expect(mocks.importConfig).toHaveBeenCalledOnce());
    expect(screen.getByLabelText("Auto-lock after (minutes, 0 = never)")).toHaveValue(42);
    expect(mocks.setThemePreference).toHaveBeenLastCalledWith("dark");

    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(mocks.setThemePreference).toHaveBeenLastCalledWith("dark");
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("applies the submitted theme even if the draft changes while Save is pending", async () => {
    let finishSave!: () => void;
    mocks.updateSettings.mockReturnValue(
      new Promise<void>((resolve) => {
        finishSave = resolve;
      }),
    );
    const onclose = vi.fn();
    render(SettingsDialog, { onclose });

    await fireEvent.click(screen.getByRole("radio", { name: "Dark" }));
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await fireEvent.click(screen.getByRole("radio", { name: "Light" }));
    finishSave();

    await waitFor(() => expect(onclose).toHaveBeenCalledOnce());
    expect(mocks.setThemePreference).toHaveBeenLastCalledWith("dark");
  });
});
