import assert from "node:assert/strict";
import { lstat, readdir, readFile } from "node:fs/promises";
import { join, relative } from "node:path";

import { createFreshVault } from "../../support/app";
import { createConnection, openConnection, waitForConnected } from "../../support/connections";
import { fileOption, openFileOption } from "../../support/files";
import { fixtures } from "../../support/fixtures";

async function treeSnapshot(root: string): Promise<string[]> {
  const snapshot: string[] = [];
  const pending = [root];
  while (pending.length > 0) {
    const directory = pending.pop();
    if (!directory) continue;
    for (const entry of await readdir(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      const name = relative(root, path);
      const metadata = await lstat(path);
      const identity = [
        entry.isDirectory() ? "directory" : "file",
        name,
        metadata.mode,
        metadata.size,
        metadata.mtimeMs,
        metadata.ctimeMs,
        metadata.ino,
      ].join(":");
      if (entry.isDirectory()) {
        snapshot.push(identity);
        pending.push(path);
      } else {
        snapshot.push(`${identity}:${(await readFile(path)).toString("base64")}`);
      }
    }
  }
  return snapshot.sort();
}

async function expectStatus(
  side: "local" | "remote",
  name: string,
  status: string,
): Promise<void> {
  const option = await fileOption(side, name);
  await option.waitForDisplayed();
  assert.equal(await option.getAttribute("data-comparison-status"), status);
}

describe("@directory-comparison", () => {
  it("classifies open folders, filters matching rows, and leaves both trees unchanged", async () => {
    const fixture = fixtures();
    const localRoot = join(fixture.paths.local_source, "directory-comparison");
    const remoteRoot = join(fixture.paths.ftp_root, "directory-comparison");
    const before = await Promise.all([treeSnapshot(localRoot), treeSnapshot(remoteRoot)]);

    await createFreshVault("directory-comparison");
    await createConnection({
      name: "Comparison FTP",
      protocol: "ftp",
      host: fixture.ftp.host,
      port: fixture.ftp.port,
      username: fixture.ftp.username,
      localDir: localRoot,
    });
    await openConnection("Comparison FTP");
    await waitForConnected();
    await openFileOption("remote", fileOption("remote", "directory-comparison"));
    await fileOption("local", "only-local.txt").waitForDisplayed();
    await fileOption("remote", "only-remote.txt").waitForDisplayed();

    const compare = await $("aria/Compare Folders");
    await compare.click();
    await compare.waitUntil(async () => (await compare.getAttribute("aria-pressed")) === "true", {
      timeoutMsg: "The comparison control did not become active.",
    });

    const summary = await $("[data-testid='directory-comparison-summary']");
    await summary.waitForDisplayed();
    await summary.waitUntil(
      async () =>
        (await summary.getAttribute("data-local-only")) === "1" &&
        (await summary.getAttribute("data-different")) === "3" &&
        (await summary.getAttribute("data-remote-only")) === "1" &&
        (await summary.getAttribute("data-matching")) === "2",
      { timeoutMsg: "The comparison summary did not reach the expected fixture totals." },
    );

    await expectStatus("local", "identical.txt", "matching");
    await expectStatus("remote", "identical.txt", "matching");
    await expectStatus("local", "shared-folder", "matching");
    await expectStatus("remote", "shared-folder", "matching");
    await expectStatus("local", "different-size.txt", "different");
    await expectStatus("remote", "different-size.txt", "different");
    await expectStatus("local", "different-date.txt", "different");
    await expectStatus("remote", "different-date.txt", "different");
    await expectStatus("local", "type-changed", "different");
    await expectStatus("remote", "type-changed", "different");
    await expectStatus("local", "only-local.txt", "local-only");
    await expectStatus("remote", "only-remote.txt", "remote-only");

    await $("aria/Differences Only").click();
    await fileOption("local", "identical.txt").waitForDisplayed({ reverse: true });
    await fileOption("remote", "shared-folder").waitForDisplayed({ reverse: true });
    await fileOption("local", "different-size.txt").waitForDisplayed();
    await fileOption("remote", "only-remote.txt").waitForDisplayed();

    assert.equal(await $("[data-testid='transfer-summary']").isExisting(), false);

    await compare.click();
    await fileOption("local", "identical.txt").waitForDisplayed();
    assert.equal(await fileOption("local", "identical.txt").getAttribute("data-comparison-status"), null);
    assert.equal(await $("aria/Differences Only").isExisting(), false);

    assert.deepEqual(
      await Promise.all([treeSnapshot(localRoot), treeSnapshot(remoteRoot)]),
      before,
      "Directory comparison must not modify either fixture tree.",
    );
  });
});
