import assert from "node:assert/strict";
import { join, win32 } from "node:path";
import { describe, it } from "node:test";

import {
  appBinaryPath,
  cargoTargetDirectory,
  fixtureBinaryPath,
  scenarioResultFile,
  scenarioTargetDirectory,
} from "./runtime-paths";

describe("runtime paths", () => {
  it("uses Cargo metadata instead of assuming an in-tree target directory", () => {
    const metadata = JSON.stringify({ target_directory: "/tmp/serverus-target" });

    assert.equal(cargoTargetDirectory(metadata), "/tmp/serverus-target");
    assert.equal(
      scenarioTargetDirectory("/tmp/serverus-target"),
      join("/tmp/serverus-target", "scenario-tests"),
    );
    assert.equal(appBinaryPath("/tmp/serverus-target", "darwin"), "/tmp/serverus-target/debug/serverus");
    assert.equal(
      fixtureBinaryPath("/tmp/serverus-target", "darwin"),
      "/tmp/serverus-target/debug/serverus-e2e-fixtures",
    );
  });

  it("adds the executable suffix on Windows", () => {
    assert.equal(
      appBinaryPath("C:\\target", "win32"),
      win32.join("C:\\target", "debug", "serverus.exe"),
    );
    assert.equal(
      fixtureBinaryPath("C:\\target", "win32"),
      win32.join("C:\\target", "debug", "serverus-e2e-fixtures.exe"),
    );
  });

  it("isolates result accounting between concurrent scenario runners", () => {
    assert.equal(
      scenarioResultFile("/tmp/scenario-artifacts", 4321),
      join("/tmp/scenario-artifacts", "results-4321.jsonl"),
    );
  });

  it("rejects invalid cargo metadata", () => {
    assert.throws(() => cargoTargetDirectory("{}"), /target_directory/);
    assert.throws(() => cargoTargetDirectory("not json"), /valid JSON/);
  });
});
