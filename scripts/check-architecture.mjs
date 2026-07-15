#!/usr/bin/env node

import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import {
  checkInternalDependencyDirection,
  checkPackageDependencies,
  checkWorkspace,
  loadCargoMetadata,
} from "./architecture/cargo-checks.mjs";
import { checkFrontend } from "./architecture/frontend-checks.mjs";
import {
  checkCoreSourcePolicy,
  checkDesktopState,
  checkRustTestPolicy,
} from "./architecture/rust-checks.mjs";
import { checkSourceFileSizes } from "./architecture/size-checks.mjs";

export function checkArchitecture(root) {
  const metadata = loadCargoMetadata(root);
  const workspace = checkWorkspace(metadata, root);
  const frontend = checkFrontend(root);
  const sizes = checkSourceFileSizes(root);

  return {
    errors: [
      ...workspace.errors,
      ...checkPackageDependencies(root, workspace.packages),
      ...checkInternalDependencyDirection(root, workspace.packages),
      ...checkCoreSourcePolicy(root, workspace.packages),
      ...checkRustTestPolicy(root, workspace.packages),
      ...checkDesktopState(root),
      ...frontend.errors,
      ...sizes.errors,
    ],
    checkedFrontendFiles: frontend.checkedFiles,
    checkedSourceFiles: sizes.checkedFiles,
    workspaceMemberCount: workspace.packages.length,
  };
}

function parseRoot(args) {
  if (args.length === 0) return process.cwd();
  if (args.length === 2 && args[0] === "--root") return resolve(args[1]);
  throw new Error("Usage: node scripts/check-architecture.mjs [--root <workspace-path>]");
}

function main() {
  try {
    const result = checkArchitecture(parseRoot(process.argv.slice(2)));
    if (result.errors.length > 0) {
      console.error(`Architecture boundary violations (${result.errors.length}):`);
      for (const error of result.errors) console.error(`- ${error}`);
      process.exitCode = 1;
      return;
    }
    console.log(
      `Architecture boundaries OK (${result.workspaceMemberCount} Cargo workspace members, ` +
        `${result.checkedFrontendFiles} frontend files, ` +
        `${result.checkedSourceFiles} handwritten source files checked).`,
    );
  } catch (error) {
    console.error(`[architecture-check] ${error instanceof Error ? error.message : String(error)}`);
    process.exitCode = 2;
  }
}

if (import.meta.url === pathToFileURL(process.argv[1]).href) main();
