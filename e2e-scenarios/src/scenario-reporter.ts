import ScenarioReporterBase, { type SuiteStats } from "@wdio/reporter";

import { appendFileSync } from "node:fs";

import type { ScenarioResult, ScenarioResultStatus } from "./scenario-results";

interface SuiteSnapshot {
  title: string;
  duration: number;
  hooks: readonly {
    state?: "failed" | "passed";
    error?: unknown;
  }[];
  tests: readonly {
    state: "pending" | "passed" | "skipped" | "failed";
    pendingReason?: string;
  }[];
  suites?: readonly SuiteSnapshot[];
}

function suiteStatus(suite: SuiteSnapshot): ScenarioResultStatus {
  const descendants = [suite, ...(suite.suites ?? []).flatMap(collectSuites)];
  if (descendants.some(({ hooks }) => hooks.some(({ state }) => state === "failed"))) {
    return "failed";
  }
  const tests = descendants.flatMap(({ tests }) => tests);
  if (tests.length === 0) return "failed";
  if (tests.every(({ state }) => state === "passed")) return "passed";
  if (tests.every(({ state }) => state === "skipped")) return "skipped";
  return "failed";
}

function collectSuites(suite: SuiteSnapshot): SuiteSnapshot[] {
  return [suite, ...(suite.suites ?? []).flatMap(collectSuites)];
}

export function scenarioResultFromSuite(suite: SuiteSnapshot): ScenarioResult | null {
  if (!/^@[a-z0-9]+(?:-[a-z0-9]+)*$/u.test(suite.title)) return null;
  return {
    scenarioId: suite.title.slice(1),
    status: suiteStatus(suite),
    durationMs: Math.max(0, Math.round(suite.duration)),
  };
}

type ReporterOptions = ConstructorParameters<typeof ScenarioReporterBase>[0] & {
  resultFile?: string;
};

export default class ScenarioReporter extends ScenarioReporterBase {
  private readonly resultFile: string;

  constructor(options: ReporterOptions) {
    super({ ...options, stdout: true });
    if (typeof options.resultFile !== "string" || options.resultFile.length === 0) {
      throw new Error("ScenarioReporter requires a resultFile option.");
    }
    this.resultFile = options.resultFile;
  }

  override onSuiteEnd(suite: SuiteStats): void {
    const result = scenarioResultFromSuite(suite);
    if (result === null) return;
    appendFileSync(this.resultFile, `${JSON.stringify(result)}\n`, { encoding: "utf8" });
  }
}
