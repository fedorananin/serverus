type ScenarioEnvironment = Partial<
  Record<"E2E_SCENARIOS" | "E2E_SCENARIO_SHARD_INDEX" | "E2E_SCENARIO_SHARDS_TOTAL", string>
>;

function positiveInteger(name: string, value: string): number {
  if (!/^\d+$/.test(value)) {
    throw new Error(`${name} must be a positive integer.`);
  }
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1) {
    throw new Error(`${name} must be a positive integer.`);
  }
  return parsed;
}

export function resolveScenarioIds<const T extends string>(
  catalog: readonly T[],
  env: ScenarioEnvironment,
): T[] {
  const requested = (env.E2E_SCENARIOS ?? "")
    .split(",")
    .map((value) => value.trim())
    .filter(Boolean);
  const available = new Set<string>(catalog);

  for (const scenario of requested) {
    if (!available.has(scenario)) {
      throw new Error(
        `Unknown E2E scenario "${scenario}". Available scenarios: ${catalog.join(", ")}.`,
      );
    }
  }

  const requestedSet = new Set(requested);
  const selected = requested.length === 0 ? [...catalog] : catalog.filter((id) => requestedSet.has(id));
  const rawIndex = env.E2E_SCENARIO_SHARD_INDEX?.trim();
  const rawTotal = env.E2E_SCENARIO_SHARDS_TOTAL?.trim();

  if (Boolean(rawIndex) !== Boolean(rawTotal)) {
    throw new Error(
      "E2E scenario sharding requires both E2E_SCENARIO_SHARD_INDEX and E2E_SCENARIO_SHARDS_TOTAL.",
    );
  }
  if (!rawIndex || !rawTotal) return selected;

  const index = positiveInteger("E2E_SCENARIO_SHARD_INDEX", rawIndex);
  const total = positiveInteger("E2E_SCENARIO_SHARDS_TOTAL", rawTotal);
  if (index > total) {
    throw new Error("E2E_SCENARIO_SHARD_INDEX must not exceed E2E_SCENARIO_SHARDS_TOTAL.");
  }

  const baseSize = Math.floor(selected.length / total);
  const largerShards = selected.length % total;
  const offset = (index - 1) * baseSize + Math.min(index - 1, largerShards);
  const size = baseSize + (index <= largerShards ? 1 : 0);
  const shard = selected.slice(offset, offset + size);
  if (shard.length === 0) {
    throw new Error(`E2E scenario shard ${index}/${total} selects no scenarios.`);
  }
  return shard;
}
