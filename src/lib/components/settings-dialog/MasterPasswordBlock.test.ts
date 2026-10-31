// @vitest-environment jsdom

import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { commands } from "$lib/api";
import MasterPasswordBlock from "./MasterPasswordBlock.svelte";
import SecuritySection from "./SecuritySection.svelte";

const mocks = vi.hoisted(() => ({
  refreshInfo: vi.fn(),
  passwordless: false,
  quickUnlockReady: false,
}));

vi.mock("$lib/api", () => ({
  commands: { vaultChangePassword: vi.fn(), vaultRemovePasswordQuick: vi.fn() },
  errorMessage: (error: unknown) => (error instanceof Error ? error.message : String(error)),
  unwrap: async (promise: Promise<{ status: "ok"; data: unknown }>) => (await promise).data,
}));

vi.mock("$lib/stores/vault.svelte", () => ({
  vault: {
    get info() {
      return {
        passwordless: mocks.passwordless,
        quick_unlock_ready: mocks.quickUnlockReady,
        quick_unlock_method: "Touch ID",
      };
    },
    refreshInfo: mocks.refreshInfo,
  },
}));

beforeEach(() => {
  mocks.passwordless = false;
  mocks.quickUnlockReady = false;
  mocks.refreshInfo.mockReset();
  vi.mocked(commands.vaultRemovePasswordQuick).mockReset();
  vi.mocked(commands.vaultRemovePasswordQuick).mockResolvedValue({ status: "ok", data: null });
  vi.mocked(commands.vaultChangePassword).mockReset();
  vi.mocked(commands.vaultChangePassword).mockResolvedValue({ status: "ok", data: null });
});

describe("master password settings", () => {
  it("removes the password only after confirming it in a dialog", async () => {
    render(MasterPasswordBlock);
    await fireEvent.click(screen.getByRole("button", { name: "Remove master password…" }));
    expect(commands.vaultChangePassword).not.toHaveBeenCalled();

    const confirm = screen.getByRole("button", { name: "Remove password" });
    expect(confirm).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Use Touch ID" })).toBeNull();
    await fireEvent.input(screen.getByPlaceholderText("Current master password"), {
      target: { value: "old" },
    });
    await fireEvent.click(confirm);

    expect(commands.vaultChangePassword).toHaveBeenCalledWith("old", "");
    await waitFor(() => expect(mocks.refreshInfo).toHaveBeenCalledOnce());
    expect(await screen.findByText("Master password removed ✓")).toBeTruthy();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("can confirm the removal with Touch ID instead of the password", async () => {
    mocks.quickUnlockReady = true;
    render(MasterPasswordBlock);
    await fireEvent.click(screen.getByRole("button", { name: "Remove master password…" }));
    await fireEvent.click(screen.getByRole("button", { name: "Use Touch ID" }));

    expect(commands.vaultRemovePasswordQuick).toHaveBeenCalledOnce();
    expect(commands.vaultChangePassword).not.toHaveBeenCalled();
    expect(await screen.findByText("Master password removed ✓")).toBeTruthy();
  });

  it("keeps the dialog open and says why when the password is wrong", async () => {
    vi.mocked(commands.vaultChangePassword).mockRejectedValue(new Error("Invalid password"));
    render(MasterPasswordBlock);
    await fireEvent.click(screen.getByRole("button", { name: "Remove master password…" }));
    await fireEvent.input(screen.getByPlaceholderText("Current master password"), {
      target: { value: "nope" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Remove password" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Invalid password");
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(mocks.refreshInfo).not.toHaveBeenCalled();
  });

  it("sets a password on a vault without one, asking for no current password", async () => {
    mocks.passwordless = true;
    render(MasterPasswordBlock);

    expect(screen.queryByPlaceholderText("Current password")).toBeNull();
    await fireEvent.input(screen.getByPlaceholderText("New password"), {
      target: { value: "fresh" },
    });
    await fireEvent.input(screen.getByPlaceholderText("Repeat new"), {
      target: { value: "fresh" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Set master password" }));

    expect(commands.vaultChangePassword).toHaveBeenCalledWith("", "fresh");
    expect(await screen.findByText("Master password set ✓")).toBeTruthy();
  });

  it("parks the lock settings while the vault has no master password", () => {
    render(SecuritySection, {
      value: { auto_lock_minutes: 15, lock_on_sleep: true, touch_id: true },
      biometryAvailable: true,
      quickUnlockMethod: "Touch ID",
      passwordless: true,
    });

    expect(screen.getByLabelText("Auto-lock after (minutes, 0 = never)")).toBeDisabled();
    expect(screen.getByLabelText("Lock when the computer sleeps")).toBeDisabled();
    expect(screen.getByLabelText("Unlock with Touch ID")).toBeDisabled();
  });
});
