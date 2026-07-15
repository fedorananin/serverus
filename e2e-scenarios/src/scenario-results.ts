import type { ScenarioPlatform } from "./scenarios";

export type ScenarioResultStatus = "passed" | "failed" | "skipped";

export interface ScenarioResult {
  scenarioId: string;
  status: ScenarioResultStatus;
  durationMs: number;
}

interface SelectedScenario {
  id: string;
  platforms: readonly ScenarioPlatform[];
}

const RESULT_FIELDS = new Set(["scenarioId", "status", "durationMs"]);
const RESULT_STATUSES = new Set<ScenarioResultStatus>(["passed", "failed", "skipped"]);

function parseResult(value: unknown, lineNumber: number): ScenarioResult {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`scenario result line ${lineNumber} must be an object`);
  }

  const record = value as Record<string, unknown>;
  for (const field of Object.keys(record)) {
    if (!RESULT_FIELDS.has(field)) {
      throw new Error(`scenario result line ${lineNumber} has unexpected field ${field}`);
    }
  }
  for (const field of RESULT_FIELDS) {
    if (!(field in record)) {
      throw new Error(`scenario result line ${lineNumber} is missing field ${field}`);
    }
  }

  if (typeof record.scenarioId !== "string" || record.scenarioId.length === 0) {
    throw new Error(`scenario result line ${lineNumber} has an invalid scenarioId`);
  }
  if (!RESULT_STATUSES.has(record.status as ScenarioResultStatus)) {
    throw new Error(`scenario result line ${lineNumber} has an invalid status`);
  }
  if (
    typeof record.durationMs !== "number" ||
    !Number.isFinite(record.durationMs) ||
    record.durationMs < 0
  ) {
    throw new Error(`scenario result line ${lineNumber} has an invalid durationMs`);
  }

  return {
    scenarioId: record.scenarioId,
    status: record.status as ScenarioResultStatus,
    durationMs: record.durationMs,
  };
}

export function parseScenarioResults(contents: string): ScenarioResult[] {
  return contents
    .split(/\r?\n/u)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line, index) => {
      try {
        return parseResult(JSON.parse(line) as unknown, index + 1);
      } catch (error) {
        if (error instanceof SyntaxError) {
          throw new Error(`scenario result line ${index + 1} is not valid JSON`);
        }
        throw error;
      }
    });
}

export function validateScenarioResults(
  selected: readonly SelectedScenario[],
  platform: ScenarioPlatform,
  results: readonly ScenarioResult[],
): string[] {
  const errors: string[] = [];
  const selectedIds = new Set(selected.map(({ id }) => id));

  for (const scenario of selected) {
    const matching = results.filter(({ scenarioId }) => scenarioId === scenario.id);
    if (matching.length !== 1) {
      errors.push(`${scenario.id}: expected one result, found ${matching.length}`);
      continue;
    }

    const expectedStatus = scenario.platforms.includes(platform) ? "passed" : "skipped";
    if (matching[0].status !== expectedStatus) {
      errors.push(`${scenario.id}: expected ${expectedStatus}, got ${matching[0].status}`);
    }
  }

  for (const scenarioId of new Set(results.map(({ scenarioId }) => scenarioId))) {
    if (!selectedIds.has(scenarioId)) {
      errors.push(`${scenarioId}: reporter emitted an unselected scenario`);
    }
  }

  return errors;
}
