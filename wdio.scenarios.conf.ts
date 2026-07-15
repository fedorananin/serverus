import type { TauriCapabilities } from "@wdio/tauri-service";

import { mkdirSync } from "node:fs";
import { resolve } from "node:path";

import {
  settleWithin,
  shouldCaptureFailureDiagnostic,
} from "./e2e-scenarios/src/scenario-diagnostics";
import ScenarioReporter from "./e2e-scenarios/src/scenario-reporter";
import { resolveScenarioIds } from "./e2e-scenarios/src/scenario-selection";
import {
  DEFAULT_SCENARIO_TIMEOUT_MS,
  scenarioTimeoutMs,
} from "./e2e-scenarios/src/scenario-timeout";
import { SCENARIO_IDS } from "./e2e-scenarios/src/scenarios";

function requiredEnvironment(name: string): string {
  const value = process.env[name]?.trim();
  if (!value) throw new Error(`${name} is required. Run scenarios through npm run test:scenarios.`);
  return value;
}

const application = requiredEnvironment("SERVERUS_SCENARIO_APP_BINARY");
const embeddedPort = Number(requiredEnvironment("SERVERUS_SCENARIO_WEBDRIVER_PORT"));
if (!Number.isInteger(embeddedPort) || embeddedPort < 1) {
  throw new Error("SERVERUS_SCENARIO_WEBDRIVER_PORT must be a valid port.");
}
const resultFile = requiredEnvironment("SERVERUS_SCENARIO_RESULT_FILE");

const selected = resolveScenarioIds(SCENARIO_IDS, process.env);
const artifacts = resolve(".artifacts/scenarios");

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: selected.map((id) => resolve(`e2e-scenarios/scenarios/${id}/${id}.e2e.spec.ts`)),
  maxInstances: 1,
  capabilities: [
    {
      browserName: "tauri",
      "tauri:options": { application },
    } as TauriCapabilities,
  ],
  services: [
    [
      "@wdio/tauri-service",
      {
        appBinaryPath: application,
        driverProvider: "embedded",
        embeddedPort,
        startTimeout: 180_000,
        statusPollTimeout: 30_000,
        captureBackendLogs: false,
        captureFrontendLogs: false,
        // v1.2.0 diagnostics still probe external tauri-driver in embedded
        // mode and report a false error; keep other service noise suppressed.
        logLevel: "error",
      },
    ],
  ],
  framework: "mocha",
  // JUnit reporter is deliberately excluded: it records raw WebDriver
  // command bodies (including setValue secrets) in system-out.
  reporters: [[ScenarioReporter, { resultFile }]],
  logLevel: "silent",
  bail: 0,
  waitforTimeout: 15_000,
  connectionRetryTimeout: 120_000,
  connectionRetryCount: 0,
  // Scenarios mutate protocol fixtures, so retries require a fresh fixture
  // process rather than WDIO's in-place spec retry.
  specFileRetries: 0,
  mochaOpts: { ui: "bdd", timeout: DEFAULT_SCENARIO_TIMEOUT_MS, retries: 0 },
  beforeTest: (test, context) => {
    context.timeout(scenarioTimeoutMs(test.parent));
  },
  afterTest: async (test, _context, result) => {
    if (!shouldCaptureFailureDiagnostic(result)) return;
    mkdirSync(artifacts, { recursive: true });
    const name = test.title.replace(/[^a-z0-9]+/gi, "-").toLowerCase();
    await settleWithin(
      browser.saveScreenshot(resolve(artifacts, `${name}-${Date.now()}.png`)),
      15_000,
    );
  },
};
