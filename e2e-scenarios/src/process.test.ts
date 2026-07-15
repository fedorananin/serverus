import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { firstOutputLine, runProcess, spawnProcess, stopProcess } from "./process";

describe("scenario child processes", () => {
  it("accepts success and rejects a non-zero exit", async () => {
    await runProcess(process.execPath, ["-e", "process.exit(0)"], { timeoutMs: 1_000 });
    await assert.rejects(
      runProcess(process.execPath, ["-e", "process.exit(7)"], { timeoutMs: 1_000 }),
      /exited with 7/u,
    );
  });

  it("terminates a hung process at its watchdog deadline", async () => {
    await assert.rejects(
      runProcess(process.execPath, ["-e", "setInterval(() => undefined, 1000)"], {
        timeoutMs: 50,
      }),
      /timed out after 50 ms/u,
    );
  });

  it("force-stops a fixture that ignores graceful termination", async () => {
    const child = spawnProcess(process.execPath, [
      "-e",
      'process.on("SIGTERM", () => {}); process.stdout.write("ready\\n"); setInterval(() => {}, 1000);',
    ]);
    assert.equal(await firstOutputLine(child, 1_000), "ready");

    const started = Date.now();
    await stopProcess(child, 50);
    assert.ok(Date.now() - started < 1_000);
    assert.ok(child.exitCode !== null || child.signalCode !== null);
  });
});
