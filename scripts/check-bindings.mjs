#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

export function verifyBindings(root, generate = () => runGenerator(root)) {
  const bindingsPath = join(root, "src", "lib", "api", "bindings.ts");
  const original = readFileSync(bindingsPath);
  let generation;
  let generated;

  try {
    generation = generate();
    generated = readFileSync(bindingsPath);
  } finally {
    // Verification must be observational even when bindings are stale or the
    // generator fails after writing. `bindings:generate` is the explicit
    // mutating command.
    const current = existsSync(bindingsPath) ? readFileSync(bindingsPath) : null;
    if (!current || !current.equals(original)) {
      writeFileSync(bindingsPath, original);
    }
  }

  return {
    current: generated.equals(original),
    generation,
  };
}

function runGenerator(root) {
  return spawnSync(
    process.env.CARGO ?? "cargo",
    [
      "run",
      "--locked",
      "--quiet",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--features",
      "bindings-generator",
      "--bin",
      "generate-bindings",
    ],
    { cwd: root, stdio: "inherit" },
  );
}

function main() {
  const root = resolve(process.cwd());
  try {
    const result = verifyBindings(root);
    if (result.generation.error) {
      console.error(`Binding generation failed: ${result.generation.error.message}`);
      process.exitCode = 2;
      return;
    }
    if (result.generation.status !== 0) {
      process.exitCode = result.generation.status ?? 2;
      return;
    }
    if (!result.current) {
      console.error(
        "Generated TypeScript bindings are stale. Run `npm run bindings:generate` and commit the result.",
      );
      process.exitCode = 1;
      return;
    }
    console.log("Generated TypeScript bindings are current.");
  } catch (error) {
    console.error(`Binding verification failed: ${error instanceof Error ? error.message : error}`);
    process.exitCode = 2;
  }
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) main();
