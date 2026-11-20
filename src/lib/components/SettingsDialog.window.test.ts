// @vitest-environment jsdom

import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Settings } from "$lib/api";
import SettingsDialog from "./SettingsDialog.svelte";

const mocks = vi.hoisted(() => ({
  updateSettings: vi.fn(),
  windowRestoreGet: vi.fn(),
  windowRestoreSet: vi.fn(),
  settings: null as Settings | null,
}));

vi.mock("$lib/theme", () => ({ setThemePreference: vi.fn() }));

vi.mock("$lib/api", () => ({
  commands: {
    windowRestoreGet: mocks.windowRestoreGet,
    windowRestoreSet: mocks.windowRestoreSet,
  },
  errorMessage: (error: unknown) =>
    error instanceof Error ? error.message : String(error),
  unwrap: async (promise: Promise<{ status: "ok"; data: unknown }>) =>
    (await promise).data,
}));

vi.mock("$lib/stores/vault.svelte", () => ({
  vault: {
    get data() {
      return { settings: mocks.settings, known_hosts: {} };
    },
    info: { path: "/vaults/main.serverus", biometry_available: false },
    updateSettings: mocks.updateSettings,
  },
}));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(async () => "1.9.0"),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));

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
    panels: {
      show_hidden: false,
      hide_local_junk: true,
      size_format: "kib",
      default_local_dir: null,
    },
    agent: { enabled: false, full_access: false, create_connections: "off" },
  };
}

describe("SettingsDialog window switch", () => {
  beforeEach(() => {
    mocks.settings = initialSettings();
    mocks.updateSettings.mockReset();
    mocks.updateSettings.mockResolvedValue(undefined);
    mocks.windowRestoreGet.mockReset();
    mocks.windowRestoreGet.mockResolvedValue({ status: "ok", data: false });
    mocks.windowRestoreSet.mockReset();
    mocks.windowRestoreSet.mockResolvedValue({ status: "ok", data: null });
  });

  it("saves the remember-window switch outside the vault, only when it changed", async () => {
    const onclose = vi.fn();
    render(SettingsDialog, { onclose });
    const remember = screen.getByLabelText("Remember window size and position");
    await waitFor(() => expect(remember).not.toBeDisabled());
    expect(remember).not.toBeChecked();

    await fireEvent.click(remember);
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(onclose).toHaveBeenCalledOnce());
    expect(mocks.windowRestoreSet).toHaveBeenCalledExactlyOnceWith(true);
    expect(mocks.updateSettings).toHaveBeenCalledWith(initialSettings());
  });

  it("leaves the remember-window switch alone when it was not touched", async () => {
    mocks.windowRestoreGet.mockResolvedValue({ status: "ok", data: true });
    const onclose = vi.fn();
    render(SettingsDialog, { onclose });
    const remember = screen.getByLabelText("Remember window size and position");
    await waitFor(() => expect(remember).toBeChecked());

    await fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(onclose).toHaveBeenCalledOnce());
    expect(mocks.windowRestoreSet).not.toHaveBeenCalled();
  });

  it("keeps the switch disabled when the current choice can't be read", async () => {
    mocks.windowRestoreGet.mockRejectedValue(new Error("unreadable"));
    render(SettingsDialog, { onclose: vi.fn() });
    const remember = screen.getByLabelText("Remember window size and position");
    await waitFor(() => expect(mocks.windowRestoreGet).toHaveBeenCalled());
    expect(remember).toBeDisabled();
  });
});
