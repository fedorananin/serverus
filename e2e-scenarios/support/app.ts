import { join } from "node:path";

import { fixtures } from "./fixtures";

const VAULT_PASSWORD = "scenario-only-master-password";

export async function createFreshVault(
  scenarioId: string,
  beforeCreate?: () => Promise<void>,
): Promise<string> {
  const vaultPath = join(
    fixtures().paths.vault_dir,
    `${scenarioId}-${process.pid}-${Date.now()}.serverus`,
  );
  const pathInput = await $("aria/Vault path");
  const mainScreen = await $("[data-testid='main-screen']");
  await browser.waitUntil(
    async () => (await pathInput.isDisplayed()) || (await mainScreen.isDisplayed()),
    { timeoutMsg: "Neither the visible vault selector nor the main screen appeared." },
  );
  if (await mainScreen.isDisplayed()) {
    await $("aria/Lock vault").click();
    await pathInput.waitForDisplayed();
  }
  await pathInput.setValue(vaultPath);
  await $("button=Use path").click();
  await $("h1=Create your vault").waitForDisplayed();
  await beforeCreate?.();
  await $("aria/Master password").setValue(VAULT_PASSWORD, { mask: true });
  await $("aria/Repeat master password").setValue(VAULT_PASSWORD, { mask: true });
  await $("button=Create vault").click();
  await waitForMainScreen();
  return vaultPath;
}

export async function waitForMainScreen(): Promise<void> {
  await $("[data-testid='main-screen']").waitForDisplayed();
}

export async function lockVault(): Promise<void> {
  await $("aria/Lock vault").click();
  await $("h1=Serverus").waitForDisplayed();
}

export async function unlockVault(password = VAULT_PASSWORD): Promise<void> {
  await $("aria/Master password").setValue(password, { mask: true });
  await $("button=Unlock").click();
}

export function correctVaultPassword(): string {
  return VAULT_PASSWORD;
}
