import { SCENARIOS } from "./scenarios";

export const DEFAULT_SCENARIO_TIMEOUT_MS = 120_000;
const RUNNER_OVERHEAD_MS = 10 * 60_000;
const MINIMUM_RUNNER_TIMEOUT_MS = 15 * 60_000;

interface TimeoutScenario {
  id: string;
  timeoutMs?: number;
}

export function scenarioTimeoutMs(
  suiteTitle: string,
  catalog: readonly TimeoutScenario[] = SCENARIOS,
): number {
  const match = /^@([a-z0-9-]+)$/u.exec(suiteTitle);
  const scenario = match && catalog.find(({ id }) => id === match[1]);
  if (!scenario) throw new Error(`Cannot resolve a scenario timeout for suite ${suiteTitle}.`);
  return scenario.timeoutMs ?? DEFAULT_SCENARIO_TIMEOUT_MS;
}

export function scenarioRunnerTimeoutMs(selected: readonly TimeoutScenario[]): number {
  const testBudgets = selected.reduce(
    (total, scenario) => total + (scenario.timeoutMs ?? DEFAULT_SCENARIO_TIMEOUT_MS),
    0,
  );
  return Math.max(MINIMUM_RUNNER_TIMEOUT_MS, testBudgets + RUNNER_OVERHEAD_MS);
}
