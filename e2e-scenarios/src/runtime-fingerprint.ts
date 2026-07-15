import { createHash } from "node:crypto";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { join, relative, resolve } from "node:path";

const FINGERPRINT_FILE = ".serverus-scenario-runtime.sha256";
const FINGERPRINT_VERSION = "serverus-scenario-runtime-v1";
const OPTIONAL_RUNTIME_INPUTS = [".cargo/config.toml"] as const;

export const RUNTIME_SOURCE_INPUTS = [
  "Cargo.lock",
  "Cargo.toml",
  "index.html",
  "package-lock.json",
  "package.json",
  "public",
  "src",
  "src-tauri/Cargo.toml",
  "src-tauri/build.rs",
  "src-tauri/capabilities",
  "src-tauri/icons",
  "src-tauri/src",
  "src-tauri/tauri.conf.json",
  "src-tauri/tauri.scenarios.conf.json",
  "src-tauri/windows-app-manifest.xml",
  "svelte.config.js",
  "tsconfig.json",
  "tsconfig.node.json",
  "vite.config.ts",
  "crates",
] as const;

function filesWithin(root: string, inputs: readonly string[]): string[] {
  const files: string[] = [];
  const visit = (path: string): void => {
    const status = statSync(path);
    if (status.isFile()) {
      files.push(path);
      return;
    }
    if (!status.isDirectory()) return;
    for (const entry of readdirSync(path).sort()) visit(join(path, entry));
  };

  for (const input of inputs) {
    const path = resolve(root, input);
    if (!existsSync(path)) throw new Error(`Scenario runtime fingerprint input is missing: ${input}.`);
    visit(path);
  }
  for (const input of OPTIONAL_RUNTIME_INPUTS) {
    const path = resolve(root, input);
    if (existsSync(path)) visit(path);
  }
  return files.sort((left, right) => relative(root, left).localeCompare(relative(root, right)));
}

export function scenarioRuntimeFingerprint(
  root: string,
  inputs: readonly string[] = RUNTIME_SOURCE_INPUTS,
  runtimeIdentity = `${process.platform}/${process.arch}`,
): string {
  const hash = createHash("sha256");
  hash.update(`${FINGERPRINT_VERSION}\0${runtimeIdentity}\0`);
  for (const path of filesWithin(root, inputs)) {
    const name = relative(root, path).replaceAll("\\", "/");
    const contents = readFileSync(path);
    hash.update(`${name.length}:${name}\0${contents.length}:`);
    hash.update(contents);
    hash.update("\0");
  }
  return hash.digest("hex");
}

export function runtimeFingerprintPath(target: string): string {
  return join(target, FINGERPRINT_FILE);
}

export function recordScenarioRuntimeFingerprint(
  root: string,
  target: string,
  inputs: readonly string[] = RUNTIME_SOURCE_INPUTS,
  runtimeIdentity = `${process.platform}/${process.arch}`,
): void {
  mkdirSync(target, { recursive: true });
  writeFileSync(
    runtimeFingerprintPath(target),
    `${scenarioRuntimeFingerprint(root, inputs, runtimeIdentity)}\n`,
    "utf8",
  );
}

export function requireCurrentScenarioRuntime(
  root: string,
  target: string,
  inputs: readonly string[] = RUNTIME_SOURCE_INPUTS,
  runtimeIdentity = `${process.platform}/${process.arch}`,
): void {
  const path = runtimeFingerprintPath(target);
  const rebuild = "Run once without SERVERUS_SCENARIO_SKIP_BUILD=1 to rebuild it.";
  if (!existsSync(path)) {
    throw new Error(`Scenario runtime fingerprint is missing. ${rebuild}`);
  }
  const recorded = readFileSync(path, "utf8").trim();
  if (recorded !== scenarioRuntimeFingerprint(root, inputs, runtimeIdentity)) {
    throw new Error(`Scenario runtime sources changed after the last build. ${rebuild}`);
  }
}
