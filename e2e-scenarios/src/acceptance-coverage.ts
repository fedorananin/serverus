export interface AcceptanceOwner {
  id: string;
  acceptanceIds: readonly string[];
}

export interface AcceptanceCoverageSummary {
  automated: string[];
  manualNative: string[];
}

export interface PlatformAcceptanceOwner extends AcceptanceOwner {
  platforms: readonly string[];
}

export interface PlatformAcceptanceCoverageSummary {
  automated: string[];
  expectedSkipped: string[];
  manualNative: string[];
  notApplicable: string[];
}

type PlatformAcceptanceCategory = keyof PlatformAcceptanceCoverageSummary;

interface PlatformAcceptanceEntry {
  category: PlatformAcceptanceCategory;
  ownerId: string;
}

function platformAcceptanceEntries(
  acceptanceId: string,
  scenarios: readonly PlatformAcceptanceOwner[],
  manualNativeOwners: readonly PlatformAcceptanceOwner[],
  notApplicableOwners: readonly PlatformAcceptanceOwner[],
  platform: string,
): PlatformAcceptanceEntry[] {
  const scenarioEntries = scenarios
    .filter(({ acceptanceIds }) => acceptanceIds.includes(acceptanceId))
    .map(({ id, platforms }) => ({
      category: platforms.includes(platform) ? "automated" : "expectedSkipped",
      ownerId: id,
    }) satisfies PlatformAcceptanceEntry);
  const manualNativeEntries = manualNativeOwners
    .filter(
      ({ acceptanceIds, platforms }) =>
        acceptanceIds.includes(acceptanceId) && platforms.includes(platform),
    )
    .map(({ id }) => ({ category: "manualNative", ownerId: id }) satisfies PlatformAcceptanceEntry);
  const notApplicableEntries = notApplicableOwners
    .filter(
      ({ acceptanceIds, platforms }) =>
        acceptanceIds.includes(acceptanceId) && platforms.includes(platform),
    )
    .map(({ id }) => ({ category: "notApplicable", ownerId: id }) satisfies PlatformAcceptanceEntry);
  return [...scenarioEntries, ...manualNativeEntries, ...notApplicableEntries];
}

export function summarizePlatformAcceptanceCoverage(
  acceptanceIds: readonly string[],
  scenarios: readonly PlatformAcceptanceOwner[],
  manualNativeOwners: readonly PlatformAcceptanceOwner[],
  notApplicableOwners: readonly PlatformAcceptanceOwner[],
  platform: string,
): PlatformAcceptanceCoverageSummary {
  const automated = new Set(
    scenarios
      .filter(({ platforms }) => platforms.includes(platform))
      .flatMap(({ acceptanceIds: ids }) => ids),
  );
  const expectedSkipped = new Set(
    scenarios
      .filter(({ platforms }) => !platforms.includes(platform))
      .flatMap(({ acceptanceIds: ids }) => ids),
  );
  const manualNative = new Set(
    manualNativeOwners
      .filter(({ platforms }) => platforms.includes(platform))
      .flatMap(({ acceptanceIds: ids }) => ids),
  );
  const notApplicable = new Set(
    notApplicableOwners
      .filter(({ platforms }) => platforms.includes(platform))
      .flatMap(({ acceptanceIds: ids }) => ids),
  );
  return {
    automated: acceptanceIds.filter((id) => automated.has(id)),
    expectedSkipped: acceptanceIds.filter((id) => expectedSkipped.has(id)),
    manualNative: acceptanceIds.filter((id) => manualNative.has(id)),
    notApplicable: acceptanceIds.filter((id) => notApplicable.has(id)),
  };
}

const CATEGORY_LABELS: Record<PlatformAcceptanceCategory, string> = {
  automated: "automated",
  expectedSkipped: "expected-skip",
  manualNative: "manual-native",
  notApplicable: "not-applicable",
};

export function validatePlatformAcceptanceCoverage(
  acceptanceIds: readonly string[],
  scenarios: readonly PlatformAcceptanceOwner[],
  manualNativeOwners: readonly PlatformAcceptanceOwner[],
  notApplicableOwners: readonly PlatformAcceptanceOwner[],
  platforms: readonly string[],
): string[] {
  const errors: string[] = [];
  const known = new Set(acceptanceIds);

  for (const platform of platforms) {
    for (const acceptanceId of acceptanceIds) {
      const entries = platformAcceptanceEntries(
        acceptanceId,
        scenarios,
        manualNativeOwners,
        notApplicableOwners,
        platform,
      );
      if (entries.length === 0) {
        errors.push(`${platform}/${acceptanceId}: acceptance criterion has no platform category`);
      } else if (entries.length > 1) {
        const accounting = entries
          .map(({ category, ownerId }) => `${CATEGORY_LABELS[category]} (${ownerId})`)
          .join(", ");
        errors.push(
          `${platform}/${acceptanceId}: expected exactly one platform category, found ${accounting}`,
        );
      }
    }
  }

  for (const owner of [...scenarios, ...manualNativeOwners, ...notApplicableOwners]) {
    for (const acceptanceId of owner.acceptanceIds) {
      if (!known.has(acceptanceId)) {
        errors.push(`${owner.id}: references unknown acceptance criterion ${acceptanceId}`);
      }
    }
  }

  return errors;
}

export function summarizeAcceptanceCoverage(
  acceptanceIds: readonly string[],
  scenarios: readonly AcceptanceOwner[],
  manualNativeOwners: readonly AcceptanceOwner[],
): AcceptanceCoverageSummary {
  const automated = new Set(scenarios.flatMap(({ acceptanceIds: ids }) => ids));
  const manualNative = new Set(
    manualNativeOwners.flatMap(({ acceptanceIds: ids }) => ids),
  );
  return {
    automated: acceptanceIds.filter((id) => automated.has(id)),
    manualNative: acceptanceIds.filter((id) => manualNative.has(id)),
  };
}

export function parseDocumentedAcceptanceIds(markdown: string): string[] {
  return [...markdown.matchAll(/^##\s+(AC-\d{3}):\s+.+$/gmu)].map((match) => match[1]);
}

export function validateAcceptanceDocument(
  declaredIds: readonly string[],
  documentedIds: readonly string[],
): string[] {
  const errors: string[] = [];
  const declared = new Set(declaredIds);
  const documented = new Set(documentedIds);

  for (const id of documented) {
    const count = documentedIds.filter((candidate) => candidate === id).length;
    if (count > 1) errors.push(`${id}: acceptance document declares criterion ${count} times`);
    if (!declared.has(id)) {
      errors.push(`${id}: documented acceptance criterion is missing from the typed catalog`);
    }
  }
  for (const id of declared) {
    if (!documented.has(id)) {
      errors.push(`${id}: typed acceptance criterion is missing from the acceptance document`);
    }
  }

  return errors;
}

export function validateAcceptanceCoverage(
  acceptanceIds: readonly string[],
  scenarios: readonly AcceptanceOwner[],
  manualNativeOwners: readonly AcceptanceOwner[],
): string[] {
  const errors: string[] = [];
  const known = new Set(acceptanceIds);
  const owners = [...scenarios, ...manualNativeOwners];

  for (const acceptanceId of acceptanceIds) {
    const matching = owners.filter((owner) => owner.acceptanceIds.includes(acceptanceId));
    if (matching.length === 0) {
      errors.push(`${acceptanceId}: acceptance criterion has no scenario or manual-native owner`);
    } else if (matching.length > 1) {
      errors.push(
        `${acceptanceId}: expected exactly one owner, found ${matching.map(({ id }) => id).join(", ")}`,
      );
    }
  }

  for (const owner of owners) {
    for (const acceptanceId of owner.acceptanceIds) {
      if (!known.has(acceptanceId)) {
        errors.push(`${owner.id}: references unknown acceptance criterion ${acceptanceId}`);
      }
    }
  }

  return errors;
}
