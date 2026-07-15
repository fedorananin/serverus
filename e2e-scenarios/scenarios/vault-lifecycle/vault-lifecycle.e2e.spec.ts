import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

import {
  createFreshVault,
  correctVaultPassword,
  lockVault,
  unlockVault,
  waitForMainScreen,
} from "../../support/app";

describe("@vault-lifecycle", () => {
  it("creates, locks, rejects a wrong password, and unlocks an encrypted vault", async () => {
    const vaultPath = await createFreshVault("vault-lifecycle", async () => {
      const warning = await $("p.hint");
      await warning.waitForDisplayed();
      assert.match(await warning.getText(), /never stored/u);
      assert.match(await warning.getText(), /no way to recover it if forgotten/u);
    });
    await $("p=Double-click a connection to open a tab.").waitForDisplayed();
    const emptyCatalog = await $("[role='status']");
    await emptyCatalog.waitForDisplayed();
    assert.match(await emptyCatalog.getText(), /No connections yet\./u);
    assert.equal(await $("[role='treeitem']").isExisting(), false);
    const encryptedVault = readFileSync(vaultPath);
    assert.ok(encryptedVault.length > 0, "The vault file was empty.");
    assert.equal(encryptedVault.includes(correctVaultPassword()), false);
    assert.throws(() => JSON.parse(encryptedVault.toString("utf8")) as unknown);

    await lockVault();

    await unlockVault("wrong-scenario-password");
    const error = await $("[role='alert']");
    await error.waitForDisplayed();
    await expect(error).toHaveText("invalid master password");
    await expect($("button=Unlock")).toBeDisplayed();
    await expect($("[data-testid='main-screen']")).not.toExist();

    await unlockVault(correctVaultPassword());
    await waitForMainScreen();
    await error.waitForExist({ reverse: true });
  });
});
