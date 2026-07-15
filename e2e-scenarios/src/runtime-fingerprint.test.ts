import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, it } from "node:test";

import {
  recordScenarioRuntimeFingerprint,
  requireCurrentScenarioRuntime,
  scenarioRuntimeFingerprint,
} from "./runtime-fingerprint";

const roots: string[] = [];

function sourceTree(): string {
  const root = mkdtempSync(join(tmpdir(), "serverus-runtime-fingerprint-"));
  roots.push(root);
  mkdirSync(join(root, "src"));
  writeFileSync(join(root, "src", "z.ts"), "export const z = 1;\n");
  writeFileSync(join(root, "src", "a.ts"), "export const a = 1;\n");
  writeFileSync(join(root, "Cargo.lock"), "lock-v1\n");
  return root;
}

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { force: true, recursive: true });
});

describe("scenario runtime fingerprint", () => {
  it("is deterministic and changes with runtime source bytes", () => {
    const root = sourceTree();
    const inputs = ["src", "Cargo.lock"];
    const first = scenarioRuntimeFingerprint(root, inputs, "test/runtime");

    assert.equal(scenarioRuntimeFingerprint(root, inputs, "test/runtime"), first);
    writeFileSync(join(root, "src", "a.ts"), "export const a = 2;\n");
    assert.notEqual(scenarioRuntimeFingerprint(root, inputs, "test/runtime"), first);
  });

  it("fails closed when skip-build has no matching completed build", () => {
    const root = sourceTree();
    const target = join(root, "target");
    const inputs = ["src", "Cargo.lock"];

    assert.throws(
      () => requireCurrentScenarioRuntime(root, target, inputs, "test/runtime"),
      /fingerprint is missing/u,
    );
    recordScenarioRuntimeFingerprint(root, target, inputs, "test/runtime");
    assert.doesNotThrow(() =>
      requireCurrentScenarioRuntime(root, target, inputs, "test/runtime"),
    );

    writeFileSync(join(root, "Cargo.lock"), "lock-v2\n");
    assert.throws(
      () => requireCurrentScenarioRuntime(root, target, inputs, "test/runtime"),
      /sources changed/u,
    );
  });
});
