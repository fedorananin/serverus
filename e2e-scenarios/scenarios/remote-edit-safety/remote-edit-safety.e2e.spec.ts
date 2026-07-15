import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { join } from "node:path";

import { createFreshVault } from "../../support/app";
import { createConnection, openConnection, waitForConnected } from "../../support/connections";
import { choosePaneAction, fileOption } from "../../support/files";
import { fixtures } from "../../support/fixtures";

const SUCCESS_CONTENT = "edited successfully by scenario editor\n";
const FAILURE_ORIGINAL = "remote failure original\n";

async function configureFixtureEditor(executable: string): Promise<void> {
  await $("aria/Settings").click();
  const dialog = await $("[role='dialog'][aria-label='Settings']");
  await dialog.waitForDisplayed();

  const useDefault = await dialog.$(
    "//label[contains(., 'Open remote files with the system default app')]/input[@type='checkbox']",
  );
  if (await useDefault.isSelected()) await useDefault.click();
  const application = await dialog.$(
    "//label[span[normalize-space()='Application']]/input[@type='text']",
  );
  await application.waitForDisplayed();
  await application.setValue(executable);
  assert.equal(await application.getValue(), executable, "The visible editor field lost its value.");
  await dialog.$("button=Save").click();
  await dialog.waitForDisplayed({ reverse: true });

  await $("aria/Settings").click();
  const persisted = await $("[role='dialog'][aria-label='Settings']");
  await persisted.waitForDisplayed();
  assert.equal(
    await persisted.$("//label[span[normalize-space()='Application']]/input[@type='text']").getValue(),
    executable,
    "The editor selection did not persist through the visible Settings flow.",
  );
  await persisted.$("button=Cancel").click();
  await persisted.waitForDisplayed({ reverse: true });
}

async function waitForRemoteBytes(path: string, expected: string): Promise<void> {
  await browser.waitUntil(
    async () => {
      try {
        return (await readFile(path, "utf8")) === expected;
      } catch (error) {
        if ((error as NodeJS.ErrnoException).code === "ENOENT") return false;
        throw error;
      }
    },
    { timeout: 60_000, timeoutMsg: `Remote bytes at ${path} did not reach the expected state.` },
  );
}

async function remoteEditLitter(root: string): Promise<string[]> {
  return (await readdir(root)).filter(
    (name) => name.startsWith(".serverus-edit-") || name.startsWith(".serverus-replace-"),
  );
}

describe("@remote-edit-safety", () => {
  it("publishes a completed edit and rolls back a rejected promotion", async () => {
    const fixture = fixtures();
    const successPath = join(fixture.paths.ftp_root, "edit-success.txt");
    const failurePath = join(fixture.paths.ftp_root, "edit-failure.txt");

    await createFreshVault("remote-edit-safety");
    await configureFixtureEditor(fixture.editor.executable);
    await createConnection({
      name: "Remote edit FTP",
      protocol: "ftp",
      host: fixture.ftp.host,
      port: fixture.ftp.port,
      username: fixture.ftp.username,
      localDir: fixture.paths.local_download,
    });
    await openConnection("Remote edit FTP");
    await waitForConnected();

    const successfulEdit = await fileOption("remote", "edit-success.txt");
    await successfulEdit.waitForDisplayed();
    await choosePaneAction("remote", successfulEdit, "Edit…");
    const successToast = await $(
      "//*[@role='status' or @role='alert'][normalize-space()='Uploaded edit-success.txt ✓']",
    );
    await successToast.waitForDisplayed({ timeout: 60_000 });
    assert.equal(await successToast.getAttribute("role"), "status");
    await waitForRemoteBytes(successPath, SUCCESS_CONTENT);
    assert.deepEqual(await remoteEditLitter(fixture.paths.ftp_root), []);

    const failedEdit = await fileOption("remote", "edit-failure.txt");
    await failedEdit.waitForDisplayed();
    await choosePaneAction("remote", failedEdit, "Edit…");
    const failureToast = await $(
      "//*[@role='alert' and contains(normalize-space(), 'Upload of edit-failure.txt failed:')]",
    );
    await failureToast.waitForDisplayed({ timeout: 60_000 });
    assert.match(await failureToast.getText(), /^Upload of edit-failure\.txt failed:/u);
    await waitForRemoteBytes(failurePath, FAILURE_ORIGINAL);
    assert.equal(
      await $("//*[@role='status' and normalize-space()='Uploaded edit-failure.txt ✓']").isExisting(),
      false,
      "A failed promotion must never emit a success status.",
    );
    assert.deepEqual(await remoteEditLitter(fixture.paths.ftp_root), []);
  });
});
