import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { createFreshVault } from "../../support/app";
import { createConnection, openConnection, waitForConnected } from "../../support/connections";
import {
  choosePaneAction,
  fileOption,
  openFileOption,
  refreshPane,
  waitForCompletedTransfers,
} from "../../support/files";
import { fixtures } from "../../support/fixtures";

type TreeEntry = readonly [kind: "directory" | "file", path: string, contents?: string];

function treeSnapshot(root: string, relative = ""): TreeEntry[] {
  return readdirSync(join(root, relative), { withFileTypes: true })
    .sort((left, right) => left.name.localeCompare(right.name, "en"))
    .flatMap((entry) => {
      const path = relative ? `${relative}/${entry.name}` : entry.name;
      if (entry.isDirectory()) {
        return [["directory", path] as const, ...treeSnapshot(root, path)];
      }
      return [["file", path, readFileSync(join(root, path)).toString("base64")] as const];
    });
}

describe("@ftp-recursive-transfer", () => {
  it("uploads and downloads a nested directory through the real FTP queue", async () => {
    const fixture = fixtures();
    const expectedTree = treeSnapshot(join(fixture.paths.local_source, "site"));
    await createFreshVault("ftp-recursive-transfer");
    await createConnection({
      name: "Scenario FTP",
      protocol: "ftp",
      host: fixture.ftp.host,
      port: fixture.ftp.port,
      username: fixture.ftp.username,
      localDir: fixture.paths.workspace_root,
    });
    await openConnection("Scenario FTP");
    await waitForConnected();

    const localSource = await fileOption("local", "local-source");
    await localSource.waitForDisplayed();
    await openFileOption("local", localSource);
    const localSite = await fileOption("local", "site");
    await localSite.waitForDisplayed();
    await choosePaneAction("local", localSite, "Upload →");
    await waitForCompletedTransfers(3);
    await refreshPane("remote");
    await (await fileOption("remote", "site")).waitForDisplayed();

    await $("[data-pane='local'] [aria-label='Up']").click();
    const localDownload = await fileOption("local", "local-download");
    await localDownload.waitForDisplayed();
    await openFileOption("local", localDownload);
    const remoteSite = await fileOption("remote", "site");
    await choosePaneAction("remote", remoteSite, "← Download");
    await waitForCompletedTransfers(6);
    await refreshPane("local");
    await (await fileOption("local", "site")).waitForDisplayed();

    expect(treeSnapshot(join(fixture.paths.local_source, "site"))).toEqual(expectedTree);
    expect(treeSnapshot(join(fixture.paths.ftp_root, "site"))).toEqual(expectedTree);
    expect(treeSnapshot(join(fixture.paths.local_download, "site"))).toEqual(expectedTree);
  });
});
