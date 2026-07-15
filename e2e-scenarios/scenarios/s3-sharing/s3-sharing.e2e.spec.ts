import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";

import { createFreshVault, lockVault, unlockVault, waitForMainScreen } from "../../support/app";
import { createConnection, openConnection, waitForConnected } from "../../support/connections";
import {
  choosePaneAction,
  chooseSelectedPaneAction,
  fileOption,
  openFileOption,
  refreshPane,
  waitForCompletedTransfers,
} from "../../support/files";
import { fixtures, S3_TEST_ACCESS_KEY, S3_TEST_SECRET_KEY } from "../../support/fixtures";
import { pressPrimaryShortcut } from "../../support/keyboard";
import { readSystemClipboard } from "../../support/system-clipboard";

async function waitForAccess(name: string, access: "private" | "public"): Promise<void> {
  const row = await fileOption("remote", name);
  await row.waitUntil(
    async () =>
      (await row.getAttribute("data-access")) === access &&
      (await row.$(".cell.perm").getText()) === access,
    {
      timeout: 30_000,
      timeoutMsg: `${name} did not show the ${access} access badge.`,
    },
  );
}

describe("@s3-sharing", () => {
  it("keeps Ask mode while uploading privately, publishing, and copying a custom public URL", async () => {
    const fixture = fixtures();
    const batchDirectory = `sharing-batch-${process.pid}-${Date.now()}`;
    const objectName = `sharing-${process.pid}-${Date.now()}.txt`;
    const secondObjectName = `sharing-${process.pid}-${Date.now()}-second.txt`;
    const batchPath = join(fixture.paths.local_source, batchDirectory);
    await mkdir(batchPath);
    await writeFile(join(batchPath, objectName), "S3 sharing scenario\n");
    await writeFile(
      join(batchPath, secondObjectName),
      "S3 sharing batch scenario\n",
    );

    await createFreshVault("s3-sharing");
    await createConnection({
      name: "Scenario S3 sharing",
      protocol: "s3",
      host: fixture.s3.endpoint,
      port: fixture.s3.port,
      username: S3_TEST_ACCESS_KEY,
      password: S3_TEST_SECRET_KEY,
      localDir: fixture.paths.local_source,
      region: "us-east-1",
      pathStyle: true,
      uploadAcl: "ask",
      publicBaseUrl: "https://cdn.serverus.invalid/assets",
    });
    await openConnection("Scenario S3 sharing");
    await waitForConnected();

    const bucket = await fileOption("remote", "serverus-e2e");
    await bucket.waitForDisplayed();
    await openFileOption("remote", bucket);
    await (await fileOption("remote", "site")).waitForDisplayed();

    const mode = await $("[data-pane='remote'] select[title='Access for uploaded files']");
    await mode.waitUntil(async () => (await mode.getValue()) === "ask", {
      timeoutMsg: "The saved Ask upload preference was not shown.",
    });

    const localBatch = await fileOption("local", batchDirectory);
    await localBatch.waitForDisplayed();
    await openFileOption("local", localBatch);
    const localObject = await fileOption("local", objectName);
    const secondLocalObject = await fileOption("local", secondObjectName);
    await localObject.waitForDisplayed();
    await secondLocalObject.waitForDisplayed();
    await localObject.click();
    await pressPrimaryShortcut("a");
    await secondLocalObject.waitUntil(
      async () => (await secondLocalObject.getAttribute("aria-selected")) === "true",
      { timeoutMsg: "The visible two-file upload batch was not selected." },
    );
    await chooseSelectedPaneAction("local", "Upload →");

    const firstPrompt = await $("[role='dialog'][aria-label='Upload access']");
    await firstPrompt.waitForDisplayed();
    expect(await firstPrompt.getText()).toContain("Upload 2 items as private or public?");
    await firstPrompt.$("button=Private").waitForDisplayed();
    await firstPrompt.$("button=Public").waitForDisplayed();
    await firstPrompt.$("button=Cancel").click();
    await firstPrompt.waitForDisplayed({ reverse: true });
    expect(await mode.getValue()).toBe("ask");

    await chooseSelectedPaneAction("local", "Upload →");
    const uploadPrompt = await $("[role='dialog'][aria-label='Upload access']");
    await uploadPrompt.waitForDisplayed();
    await uploadPrompt.$("button=Private").click();
    await uploadPrompt.waitForDisplayed({ reverse: true });
    expect(await mode.getValue()).toBe("ask");

    // One decision must enqueue the whole selected batch. A regression that
    // prompts once per file leaves the second modal open and cannot reach
    // this exact, idle two-item summary.
    await waitForCompletedTransfers(2);
    await refreshPane("remote");
    const remoteObject = await fileOption("remote", objectName);
    const secondRemoteObject = await fileOption("remote", secondObjectName);
    await remoteObject.waitForDisplayed({ timeout: 30_000 });
    await secondRemoteObject.waitForDisplayed({ timeout: 30_000 });
    await waitForAccess(objectName, "private");
    await waitForAccess(secondObjectName, "private");

    await choosePaneAction("remote", remoteObject, "Make public");
    const publishedNote = await $("[data-pane='remote'] .acl-note");
    await publishedNote.waitUntil(
      async () => (await publishedNote.getText()) === "1 object made public",
      {
        timeout: 30_000,
        timeoutMsg: "The S3 object was not reported public.",
      },
    );
    await waitForAccess(objectName, "public");

    await choosePaneAction("remote", remoteObject, "Copy public URL");
    const copiedNote = await $("[data-pane='remote'] .acl-note[role='status']");
    await copiedNote.waitUntil(
      async () => (await copiedNote.getText()) === "Public URL copied",
      {
        timeout: 10_000,
        timeoutMsg: "The public URL clipboard write did not report success.",
      },
    );
    expect(readSystemClipboard()).toBe(
      `https://cdn.serverus.invalid/assets/serverus-e2e/${objectName}`,
    );
    expect(await mode.getValue()).toBe("ask");

    await lockVault();
    await unlockVault();
    await waitForMainScreen();
    const persistedMode = await $(
      "[data-pane='remote'] select[title='Access for uploaded files']",
    );
    await persistedMode.waitUntil(async () => (await persistedMode.getValue()) === "ask", {
      timeoutMsg: "The Ask upload preference did not survive a vault reload.",
    });
  });
});
