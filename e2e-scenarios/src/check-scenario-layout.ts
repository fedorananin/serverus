import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import {
  parseDocumentedAcceptanceIds,
  summarizeAcceptanceCoverage,
  summarizePlatformAcceptanceCoverage,
  validateAcceptanceCoverage,
  validateAcceptanceDocument,
  validatePlatformAcceptanceCoverage,
} from "./acceptance-coverage";
import { validateScenarioLayout } from "./scenario-layout";
import {
  MANUAL_NATIVE_SUPPLEMENTS,
  validateManualNativeSupplements,
} from "./scenario-supplements";
import {
  ACCEPTANCE_IDS,
  ALL_PLATFORMS,
  MANUAL_NATIVE_ACCEPTANCE,
  NOT_APPLICABLE_ACCEPTANCE,
  SCENARIOS,
  SCENARIO_IDS,
  validateManualNativeCatalog,
  validateScenarioCatalog,
} from "./scenarios";

const root = resolve("e2e-scenarios/scenarios");
const acceptanceDocument = readFileSync(
  resolve("docs/business-requirements/09-acceptance-criteria.md"),
  "utf8",
);
const errors = [
  ...validateScenarioCatalog(SCENARIOS),
  ...validateManualNativeCatalog(MANUAL_NATIVE_ACCEPTANCE, SCENARIO_IDS),
  ...validateManualNativeSupplements(
    MANUAL_NATIVE_SUPPLEMENTS,
    SCENARIOS,
    [...SCENARIO_IDS, ...MANUAL_NATIVE_ACCEPTANCE.map(({ id }) => id)],
  ),
  ...validateAcceptanceDocument(
    ACCEPTANCE_IDS,
    parseDocumentedAcceptanceIds(acceptanceDocument),
  ),
  ...validateAcceptanceCoverage(ACCEPTANCE_IDS, SCENARIOS, MANUAL_NATIVE_ACCEPTANCE),
  ...validatePlatformAcceptanceCoverage(
    ACCEPTANCE_IDS,
    SCENARIOS,
    MANUAL_NATIVE_ACCEPTANCE,
    NOT_APPLICABLE_ACCEPTANCE,
    ALL_PLATFORMS,
  ),
  ...validateScenarioLayout(root, SCENARIO_IDS),
];

function mixedAcceptanceIds(platform?: (typeof ALL_PLATFORMS)[number]): string[] {
  const mixed = new Set<string>(
    MANUAL_NATIVE_SUPPLEMENTS.filter(
      ({ coverageImpact, platforms }) =>
        coverageImpact === "required-path" && (!platform || platforms.includes(platform)),
    ).map(({ acceptanceId }) => acceptanceId),
  );
  return ACCEPTANCE_IDS.filter((id) => mixed.has(id));
}

if (errors.length > 0) {
  console.error(errors.map((error) => `- ${error}`).join("\n"));
  process.exitCode = 1;
} else {
  const coverage = summarizeAcceptanceCoverage(
    ACCEPTANCE_IDS,
    SCENARIOS,
    MANUAL_NATIVE_ACCEPTANCE,
  );
  const mixed = mixedAcceptanceIds();
  const fullyAutomated = coverage.automated.filter((id) => !mixed.includes(id));
  console.log(
    `Scenario catalog is valid (${SCENARIO_IDS.length} automated scenarios; ` +
      `${fullyAutomated.length}/${ACCEPTANCE_IDS.length} fully automated; ` +
      `${mixed.length}/${ACCEPTANCE_IDS.length} mixed automated/manual-native: ${mixed.join(", ")}; ` +
      `${coverage.manualNative.length}/${ACCEPTANCE_IDS.length} manual-native owners: ` +
      `${coverage.manualNative.join(", ")}).`,
  );
  console.log(
    `Manual-native supplements: ${MANUAL_NATIVE_SUPPLEMENTS.length} (` +
      MANUAL_NATIVE_SUPPLEMENTS.map(
        ({ id, automatedOwnerId, acceptanceId, platforms }) =>
          `${id} → ${automatedOwnerId}/${acceptanceId} on ${platforms.join(", ")}`,
      ).join("; ") +
      ").",
  );
  for (const platform of ALL_PLATFORMS) {
    const platformCoverage = summarizePlatformAcceptanceCoverage(
      ACCEPTANCE_IDS,
      SCENARIOS,
      MANUAL_NATIVE_ACCEPTANCE,
      NOT_APPLICABLE_ACCEPTANCE,
      platform,
    );
    const platformMixed = mixedAcceptanceIds(platform).filter((id) =>
      platformCoverage.automated.includes(id),
    );
    const platformFullyAutomated = platformCoverage.automated.filter(
      (id) => !platformMixed.includes(id),
    );
    console.log(
      `${platform}: ${platformFullyAutomated.length}/${ACCEPTANCE_IDS.length} fully automated; ` +
        `mixed ${platformMixed.length}` +
        `${platformMixed.length > 0 ? ` (${platformMixed.join(", ")})` : ""}; ` +
        `expected skips ${platformCoverage.expectedSkipped.length}` +
        `${platformCoverage.expectedSkipped.length > 0 ? ` (${platformCoverage.expectedSkipped.join(", ")})` : ""}; ` +
        `manual-native owners ${platformCoverage.manualNative.length}` +
        `${platformCoverage.manualNative.length > 0 ? ` (${platformCoverage.manualNative.join(", ")})` : ""}; ` +
        `not-applicable ${platformCoverage.notApplicable.length}` +
        `${platformCoverage.notApplicable.length > 0 ? ` (${platformCoverage.notApplicable.join(", ")})` : ""}.`,
    );
  }
}
