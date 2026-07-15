import assert from "node:assert/strict";

import { createFreshVault, lockVault, unlockVault, waitForMainScreen } from "../../support/app";
import { createConnection, openConnection, waitForConnected } from "../../support/connections";
import {
  currentSshHostKeyIdentity,
  fixtures,
  rotateSshHostKey,
  type SshHostKeyIdentity,
} from "../../support/fixtures";
import {
  expectTerminalText,
  pasteTerminalText,
} from "../../support/terminal";

async function expectHostKeyDetails(
  dialog: ReturnType<typeof $>,
  endpoint: string,
  identity: SshHostKeyIdentity,
): Promise<void> {
  const text = await dialog.getText();
  assert.ok(text.includes(endpoint), "Host-key dialog did not show the SSH endpoint.");
  assert.ok(text.includes(identity.algorithm), "Host-key dialog did not show the key algorithm.");
  assert.ok(text.includes(identity.fingerprint), "Host-key dialog did not show the fingerprint.");
}

async function closeActiveTab(): Promise<void> {
  await $("aria/Close tab").click();
  await $("[data-testid='session-state']").waitForExist({ reverse: true });
}

describe("@ssh-terminal", () => {
  it("persists a trusted host key and rejects a changed key", async function () {
    const fixture = fixtures();
    if (!fixture.ssh.available) this.skip();
    const endpoint = `${fixture.ssh.host}:${fixture.ssh.port}`;
    const initialKey = await currentSshHostKeyIdentity();

    await createFreshVault("ssh-terminal");
    await createConnection({
      name: "Scenario SSH",
      protocol: "ssh",
      host: fixture.ssh.host,
      port: fixture.ssh.port,
      username: fixture.ssh.username,
      authMethod: "key",
      keyPath: fixture.ssh.key_path,
    });
    await openConnection("Scenario SSH");

    const trustDialog = await $("[role='dialog'][aria-label='Unknown host']");
    await trustDialog.waitForDisplayed({ timeout: 30_000 });
    await expectHostKeyDetails(trustDialog, endpoint, initialKey);
    assert.equal(
      await $("[data-testid='session-state'][data-state='connected']").isExisting(),
      false,
      "SSH connected before the unknown host key was trusted.",
    );
    assert.equal(
      await $("[data-terminal-state]").isExisting(),
      false,
      "SSH opened a terminal before the unknown host key was trusted.",
    );
    const ordinaryBorder = await trustDialog.getCSSProperty("border-top-color");
    await trustDialog.$("button=Trust and connect").click();
    await waitForConnected();

    const terminalScreen = await $("[data-terminal-state='ready'] .xterm-screen");
    await terminalScreen.waitForDisplayed();
    await pasteTerminalText("echo SERVERUS_RESULT_$((40 + 2))\n");
    await expectTerminalText("SERVERUS_RESULT_42");

    await closeActiveTab();
    await lockVault();
    await unlockVault();
    await waitForMainScreen();
    await openConnection("Scenario SSH");
    await waitForConnected();
    assert.equal(
      await $("[role='dialog'][aria-label='Unknown host']").isExisting(),
      false,
      "The accepted host key was not reused on reconnect.",
    );

    await closeActiveTab();
    const changedKey = await rotateSshHostKey();
    assert.notEqual(changedKey.fingerprint, initialKey.fingerprint);
    await openConnection("Scenario SSH");

    const changedDialog = await $("[role='dialog'][aria-label='Host key changed']");
    await changedDialog.waitForDisplayed({ timeout: 30_000 });
    await expectHostKeyDetails(changedDialog, endpoint, changedKey);
    assert.equal(
      await $("[data-testid='session-state'][data-state='connected']").isExisting(),
      false,
      "SSH connected while the changed host key was awaiting a decision.",
    );
    assert.equal(
      await $("[data-terminal-state]").isExisting(),
      false,
      "SSH opened a terminal while the changed host key was awaiting a decision.",
    );
    assert.ok(
      (await changedDialog.getText()).includes("Do not accept unless you know why the key changed."),
      "Changed-key dialog did not show the security warning.",
    );
    const severeBorder = await changedDialog.getCSSProperty("border-top-color");
    assert.notEqual(
      severeBorder.value,
      ordinaryBorder.value,
      "Changed-key warning did not use a more severe visible border.",
    );
    await changedDialog.$("button.danger=Accept new key anyway").waitForDisplayed();
    await changedDialog.$("button=Cancel").click();

    await $("[data-testid='session-state'][data-state='error']").waitForDisplayed();
    await $("p=Host key rejected").waitForDisplayed();
    await $("[data-terminal-state]").waitForExist({ reverse: true });
  });
});
