import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { resolve } from "node:path";

import { scenarioBuildEnvironment } from "./build-environment";
import { parseFixtureManifest } from "./fixture-manifest";
import { firstOutputLine, runProcess, spawnProcess, stopProcess } from "./process";
import {
  recordScenarioRuntimeFingerprint,
  requireCurrentScenarioRuntime,
} from "./runtime-fingerprint";
import {
  appBinaryPath,
  cargoTargetDirectory,
  fixtureBinaryPath,
  scenarioResultFile,
  scenarioTargetDirectory,
} from "./runtime-paths";
import { parseScenarioResults, validateScenarioResults } from "./scenario-results";
import { resolveScenarioIds } from "./scenario-selection";
import { scenarioRunnerTimeoutMs } from "./scenario-timeout";
import { SCENARIOS, SCENARIO_IDS } from "./scenarios";

const root = process.cwd();

function scenarioPlatform(value: NodeJS.Platform): "darwin" | "linux" | "win32" {
  if (value !== "darwin" && value !== "linux" && value !== "win32") {
    throw new Error(`Unsupported scenario-test platform: ${value}.`);
  }
  return value;
}

const platform = scenarioPlatform(process.platform);

async function availablePort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolveReady, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolveReady);
  });
  const address = server.address();
  if (typeof address !== "object" || address === null) throw new Error("Failed to allocate a port.");
  await new Promise<void>((resolveClosed) => server.close(() => resolveClosed()));
  return address.port;
}

function targetDirectory(): string {
  const output = execFileSync("cargo", ["metadata", "--format-version=1", "--no-deps"], {
    cwd: root,
    encoding: "utf8",
  });
  return cargoTargetDirectory(output);
}

async function buildScenarioRuntime(target: string): Promise<void> {
  if (process.env.SERVERUS_SCENARIO_SKIP_BUILD === "1") {
    requireCurrentScenarioRuntime(root, target);
    return;
  }

  const buildEnvironment = scenarioBuildEnvironment(process.env, target);

  await runProcess(
    process.execPath,
    [
      resolve("node_modules/@tauri-apps/cli/tauri.js"),
      "build",
      "--debug",
      "--config",
      "src-tauri/tauri.scenarios.conf.json",
      "--features",
      "scenario-tests",
    ],
    { env: buildEnvironment, timeoutMs: 30 * 60_000 },
  );
  await runProcess(
    "cargo",
    ["build", "--locked", "-p", "serverus-e2e-fixtures"],
    { env: buildEnvironment, timeoutMs: 30 * 60_000 },
  );
  recordScenarioRuntimeFingerprint(root, target);
}

async function main(): Promise<void> {
  const selectedIds = resolveScenarioIds(SCENARIO_IDS, process.env);
  const selected = SCENARIOS.filter(({ id }) => selectedIds.includes(id));
  const target = scenarioTargetDirectory(targetDirectory());
  await buildScenarioRuntime(target);
  const application = appBinaryPath(target, platform);
  const fixtureExecutable = fixtureBinaryPath(target, platform);
  for (const path of [application, fixtureExecutable]) {
    if (!existsSync(path)) throw new Error(`Scenario runtime is missing: ${path}.`);
  }

  const artifacts = resolve(".artifacts/scenarios");
  const resultFile = scenarioResultFile(artifacts, process.pid);
  mkdirSync(artifacts, { recursive: true });
  rmSync(resultFile, { force: true });

  const fixture = spawnProcess(fixtureExecutable, []);
  try {
    const manifestLine = await firstOutputLine(fixture, 30_000);
    const manifest = parseFixtureManifest(manifestLine);
    const safeManifest = JSON.stringify(manifest);
    const webdriverPort = await availablePort();
    const env = {
      ...process.env,
      SERVERUS_SCENARIO_APP_BINARY: application,
      SERVERUS_SCENARIO_CONFIG_DIR: manifest.paths.app_config_dir,
      SERVERUS_SCENARIO_FIXTURE_MANIFEST: safeManifest,
      SERVERUS_SCENARIO_RESULT_FILE: resultFile,
      SERVERUS_SCENARIO_WEBDRIVER_PORT: String(webdriverPort),
      TAURI_WEBDRIVER_PORT: String(webdriverPort),
    };

    await runProcess(
      process.execPath,
      [resolve("node_modules/@wdio/cli/bin/wdio.js"), "run", "wdio.scenarios.conf.ts"],
      { env, timeoutMs: scenarioRunnerTimeoutMs(selected) },
    );

    const results = parseScenarioResults(
      existsSync(resultFile) ? readFileSync(resultFile, "utf8") : "",
    );
    const errors = validateScenarioResults(selected, platform, results);
    if (errors.length > 0) {
      throw new Error(`Scenario result accounting failed:\n${errors.map((error) => `- ${error}`).join("\n")}`);
    }
    console.log(
      results
        .map(({ scenarioId, status, durationMs }) => `${scenarioId}: ${status} (${durationMs} ms)`)
        .join("\n"),
    );
  } finally {
    await stopProcess(fixture);
  }
}

main().catch((error: unknown) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
