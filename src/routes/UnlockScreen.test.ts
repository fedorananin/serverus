// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

import UnlockScreen from "./UnlockScreen.svelte";

const vaultMock = vi.hoisted(() => ({
  info: { exists: true, quick_unlock_ready: false, path: "/current.serverus" },
  busy: false,
  error: null as string | null,
  switchVault: vi.fn(async () => {}),
  create: vi.fn(async () => true),
  unlockPassword: vi.fn(async () => true),
  unlockQuick: vi.fn(async () => true),
}));

vi.mock("$lib/stores/vault.svelte", () => ({ vault: vaultMock }));
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => null),
}));

describe("UnlockScreen vault path", () => {
  beforeEach(() => {
    vaultMock.switchVault.mockClear();
  });

  it("lets a user select a vault by typing its visible path", async () => {
    render(UnlockScreen);

    await fireEvent.input(screen.getByRole("textbox", { name: "Vault path" }), {
      target: { value: "  /tmp/scenario.serverus  " },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Use path" }));

    expect(vaultMock.switchVault).toHaveBeenCalledOnce();
    expect(vaultMock.switchVault).toHaveBeenCalledWith("/tmp/scenario.serverus");
  });
});
