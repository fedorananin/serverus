import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { createFreshVault } from "../../support/app";
import { createConnection, openConnection, waitForConnected } from "../../support/connections";
import { fixtures } from "../../support/fixtures";
import {
  enterDirectory,
  resolveConflict,
  retryTransfer,
  transferByAction,
  waitForConflict,
  waitForTransferState,
  waitForTransferSummary,
} from "./transfer-resilience.support";

const idle = (done: number, total: number) => ({
  queued: 0,
  running: 0,
  done,
  failed: 0,
  total,
});

function ftpFile(name: string): string {
  return join(fixtures().paths.ftp_root, "conflicts", name);
}

function localDownload(name: string): string {
  return join(fixtures().paths.local_source, "conflicts", name);
}

function retrievalOffsets(): number[] {
  const telemetry = join(fixtures().paths.workspace_root, "ftp-retrievals.jsonl");
  if (!existsSync(telemetry)) return [];
  const contents = readFileSync(telemetry, "utf8");
  const completeContents = contents.endsWith("\n")
    ? contents
    : contents.slice(0, Math.max(0, contents.lastIndexOf("\n") + 1));
  return completeContents
    .split(/\r?\n/u)
    .filter(Boolean)
    .map((line) => {
      const record = JSON.parse(line) as Record<string, unknown>;
      expect(Object.keys(record)).toEqual(["start_pos"]);
      expect(typeof record.start_pos).toBe("number");
      return record.start_pos as number;
    });
}

async function expectUploadConflict(
  name: string,
  action: "Overwrite" | "Skip" | "Rename",
  completed: number,
): Promise<void> {
  await transferByAction("local", name);
  await waitForConflict(name);
  await resolveConflict(action);
  await waitForTransferSummary(idle(completed, completed));
}

describe("@transfer-resilience", () => {
  it("scopes conflict decisions to one batch and resumes an interrupted FTP download", async () => {
    const fixture = fixtures();
    const expectedResumeBytes = readFileSync(ftpFile("resume.bin"));
    await createFreshVault("transfer-resilience");
    await createConnection({
      name: "Resilient FTP",
      protocol: "ftp",
      host: fixture.ftp.host,
      port: fixture.ftp.port,
      username: fixture.ftp.username,
      localDir: fixture.paths.local_source,
    });
    await openConnection("Resilient FTP");
    await waitForConnected();
    await enterDirectory("local", "conflicts");
    await enterDirectory("remote", "conflicts");

    await expectUploadConflict("overwrite.txt", "Overwrite", 1);
    expect(readFileSync(ftpFile("overwrite.txt"), "utf8")).toBe("local overwrite\n");

    await expectUploadConflict("skip.txt", "Skip", 2);
    expect(readFileSync(ftpFile("skip.txt"), "utf8")).toBe("remote skip\n");

    await expectUploadConflict("rename.txt", "Rename", 3);
    expect(readFileSync(ftpFile("rename.txt"), "utf8")).toBe("remote rename\n");
    expect(readFileSync(ftpFile("rename (1).txt"), "utf8")).toBe("local rename\n");

    await transferByAction("local", "batch");
    await waitForTransferState("batch-a.txt", "conflict");
    await waitForTransferState("batch-b.txt", "conflict");
    await resolveConflict("Overwrite", true);
    await waitForTransferSummary(idle(5, 5));
    expect(readFileSync(ftpFile("batch/batch-a.txt"), "utf8")).toBe("local batch a\n");
    expect(readFileSync(ftpFile("batch/batch-b.txt"), "utf8")).toBe("local batch b\n");

    await transferByAction("local", "batch-skip");
    await waitForTransferState("batch-a.txt", "conflict");
    await waitForTransferState("batch-b.txt", "conflict");
    await resolveConflict("Skip", true);
    await waitForTransferSummary(idle(7, 7));
    expect(readFileSync(ftpFile("batch-skip/batch-a.txt"), "utf8")).toBe(
      "remote skip batch a\n",
    );
    expect(readFileSync(ftpFile("batch-skip/batch-b.txt"), "utf8")).toBe(
      "remote skip batch b\n",
    );

    await transferByAction("local", "batch-rename");
    await waitForTransferState("batch-a.txt", "conflict");
    await waitForTransferState("batch-b.txt", "conflict");
    await resolveConflict("Rename", true);
    await waitForTransferSummary(idle(9, 9));
    expect(readFileSync(ftpFile("batch-rename/batch-a.txt"), "utf8")).toBe(
      "remote rename batch a\n",
    );
    expect(readFileSync(ftpFile("batch-rename/batch-a (1).txt"), "utf8")).toBe(
      "local rename batch a\n",
    );
    expect(readFileSync(ftpFile("batch-rename/batch-b.txt"), "utf8")).toBe(
      "remote rename batch b\n",
    );
    expect(readFileSync(ftpFile("batch-rename/batch-b (1).txt"), "utf8")).toBe(
      "local rename batch b\n",
    );

    await transferByAction("local", "after-batch.txt");
    await waitForConflict("after-batch.txt");
    await resolveConflict("Skip");
    await waitForTransferSummary(idle(10, 10));
    expect(readFileSync(ftpFile("after-batch.txt"), "utf8")).toBe("remote after batch\n");

    await transferByAction("remote", "resume.bin");
    await browser.waitUntil(() => retrievalOffsets().length >= 3, {
      timeout: 60_000,
      timeoutMsg: "FTP fixture did not observe all automatic resume attempts.",
    });
    await waitForTransferState("resume.bin", "error");
    await waitForTransferSummary({ queued: 0, running: 0, done: 10, failed: 1, total: 11 });

    await retryTransfer("resume.bin");
    await waitForTransferSummary(idle(11, 11));
    expect(readFileSync(ftpFile("resume.bin"))).toEqual(expectedResumeBytes);
    expect(readFileSync(localDownload("resume.bin"))).toEqual(expectedResumeBytes);

    const offsets = retrievalOffsets();
    expect(offsets).toHaveLength(4);
    expect(offsets[0]).toBe(0);
    expect(offsets.some((offset) => offset > 0)).toBe(true);
    expect(offsets.every((offset, index) => index === 0 || offset >= offsets[index - 1])).toBe(true);
  });
});
