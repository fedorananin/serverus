import { join } from "node:path";

type SupportedPlatform = "darwin" | "linux" | "win32";

export function cargoTargetDirectory(metadataOutput: string): string {
  let metadata: unknown;
  try {
    metadata = JSON.parse(metadataOutput);
  } catch {
    throw new Error("cargo metadata did not return valid JSON.");
  }

  if (
    typeof metadata !== "object" ||
    metadata === null ||
    !("target_directory" in metadata) ||
    typeof metadata.target_directory !== "string" ||
    metadata.target_directory.length === 0
  ) {
    throw new Error("cargo metadata is missing target_directory.");
  }
  return metadata.target_directory;
}

export function scenarioTargetDirectory(cargoTarget: string): string {
  return join(cargoTarget, "scenario-tests");
}

export function scenarioResultFile(artifactsDirectory: string, processId: number): string {
  return join(artifactsDirectory, `results-${processId}.jsonl`);
}

function debugBinaryPath(
  targetDirectory: string,
  name: string,
  platform: SupportedPlatform,
): string {
  const suffix = platform === "win32" ? ".exe" : "";
  return `${targetDirectory}/debug/${name}${suffix}`;
}

export function appBinaryPath(targetDirectory: string, platform: SupportedPlatform): string {
  return debugBinaryPath(targetDirectory, "serverus", platform);
}

export function fixtureBinaryPath(targetDirectory: string, platform: SupportedPlatform): string {
  return debugBinaryPath(targetDirectory, "serverus-e2e-fixtures", platform);
}
