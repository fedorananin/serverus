// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

import UnlockScreen from "./UnlockScreen.svelte";

const vaultMock = vi.hoisted(() => ({
  info: {
    exists: true,
    quick_unlock_ready: false,
    passwordless: false,
    path: "/current.serverus",
    scenario_build: true,
  },
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
    vaultMock.info.scenario_build = true;
  });

  it("lets a scenario build select a vault by typing its visible path", async () => {
    render(UnlockScreen);

    await fireEvent.input(screen.getByRole("textbox", { name: "Vault path" }), {
      target: { value: "  /tmp/scenario.serverus  " },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Use path" }));

    expect(vaultMock.switchVault).toHaveBeenCalledOnce();
    expect(vaultMock.switchVault).toHaveBeenCalledWith("/tmp/scenario.serverus");
  });

  it("hides the typed path field outside scenario builds", () => {
    vaultMock.info.scenario_build = false;

    render(UnlockScreen);

    expect(screen.queryByRole("textbox", { name: "Vault path" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Use path" })).toBeNull();
    // The native pickers remain the user-facing way to re-point the app.
    expect(screen.getByRole("button", { name: "Open another vault…" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "New vault…" })).toBeTruthy();
  });
});

describe("UnlockScreen without a master password", () => {
  beforeEach(() => {
    vaultMock.create.mockClear();
    vaultMock.unlockPassword.mockClear();
    vaultMock.info.exists = true;
    vaultMock.info.passwordless = false;
  });

  it("lets a new vault skip the master password", async () => {
    vaultMock.info.exists = false;
    render(UnlockScreen);

    await fireEvent.click(
      screen.getByRole("button", { name: "Continue without a master password" }),
    );

    expect(vaultMock.create).toHaveBeenCalledWith("");
  });

  it("opens a passwordless vault with one click and no password field", async () => {
    vaultMock.info.passwordless = true;
    render(UnlockScreen);

    expect(screen.queryByLabelText("Master password")).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Open vault" }));

    expect(vaultMock.unlockPassword).toHaveBeenCalledWith("");
  });
});
