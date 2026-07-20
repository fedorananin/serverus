#!/usr/bin/env node
// Wipe the Cargo build cache, including the scenario-tests tree that
// `cargo clean` does not know about (run-scenarios.ts builds into a nested
// target dir). The cache is fully reproducible: the next build rebuilds it.
//
// Cargo never garbage-collects stale artifacts — every changed flag, feature
// or dependency version leaves its old hash-suffixed output behind forever —
// so this needs running by hand every few weeks of active work.
import { execFileSync } from "node:child_process";
import { rmSync, statSync } from "node:fs";
import { join } from "node:path";

function targetDirectory() {
  const output = execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  const { target_directory: dir } = JSON.parse(output);
  if (typeof dir !== "string" || dir.length === 0) {
    throw new Error("cargo metadata is missing target_directory.");
  }
  return dir;
}

function sizeOf(dir) {
  try {
    statSync(dir);
  } catch {
    return "0B";
  }
  return execFileSync("du", ["-sh", dir], { encoding: "utf8" }).split("\t")[0].trim();
}

const target = targetDirectory();
console.log(`Cargo target directory: ${target} (${sizeOf(target)})`);

// The scenario tree lives inside the target dir, so removing the target dir
// covers it — but keep this explicit in case the layout ever changes.
rmSync(join(target, "scenario-tests"), { recursive: true, force: true });
rmSync(target, { recursive: true, force: true });

console.log("Build cache removed. The next `cargo build` will be a cold one.");
