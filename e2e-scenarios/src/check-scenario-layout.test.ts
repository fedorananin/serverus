import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { describe, it } from "node:test";

describe("scenario layout command", () => {
  it("prints exhaustive per-platform acceptance accounting", () => {
    const result = spawnSync(
      process.execPath,
      ["--import", "tsx", "e2e-scenarios/src/check-scenario-layout.ts"],
      { cwd: process.cwd(), encoding: "utf8" },
    );

    assert.equal(result.status, 0, result.stderr);
    assert.match(
      result.stdout,
      /darwin: 14\/17 fully automated; mixed 1 \(AC-017\); expected skips 0; manual-native owners 2 \(AC-002, AC-015\); not-applicable 0\./,
    );
    assert.match(
      result.stdout,
      /linux: 14\/17 fully automated; mixed 1 \(AC-017\); expected skips 0; manual-native owners 1 \(AC-015\); not-applicable 1 \(AC-002\)\./,
    );
    assert.match(
      result.stdout,
      /win32: 9\/17 fully automated; mixed 1 \(AC-017\); expected skips 5 \(AC-003, AC-005, AC-013, AC-014, AC-016\); manual-native owners 2 \(AC-002, AC-015\); not-applicable 0\./,
    );
    assert.match(
      result.stdout,
      /14\/17 fully automated; 1\/17 mixed automated\/manual-native: AC-017/,
    );
    assert.match(
      result.stdout,
      /Manual-native supplements: 4 \(/,
    );
    assert.match(result.stdout, /platform-shortcuts-arrow-transfer-native → platform-shortcuts\/AC-017 on darwin, linux, win32/);
    assert.match(result.stdout, /platform-keyboard-context-menu-native → platform-shortcuts\/AC-017 on darwin, linux, win32/);
    assert.match(result.stdout, /platform-context-menu-native → platform-shortcuts\/AC-017 on darwin, linux, win32/);
    assert.match(result.stdout, /remote-edit-native-editor → remote-edit-safety\/AC-009 on darwin, linux, win32/);
  });
});
