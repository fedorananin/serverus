function positiveJobCount(value: string): string {
  const trimmed = value.trim();
  if (!/^\d+$/u.test(trimmed) || Number(trimmed) < 1 || !Number.isSafeInteger(Number(trimmed))) {
    throw new Error("SERVERUS_SCENARIO_BUILD_JOBS must be a positive integer.");
  }
  return trimmed;
}

export function scenarioBuildEnvironment(
  environment: NodeJS.ProcessEnv,
  targetDirectory: string,
): NodeJS.ProcessEnv {
  const explicitCargoJobs = environment.CARGO_BUILD_JOBS?.trim();
  const scenarioJobs = environment.SERVERUS_SCENARIO_BUILD_JOBS;
  return {
    ...environment,
    CARGO_BUILD_JOBS:
      explicitCargoJobs || (scenarioJobs === undefined ? "1" : positiveJobCount(scenarioJobs)),
    CARGO_TARGET_DIR: targetDirectory,
  };
}
