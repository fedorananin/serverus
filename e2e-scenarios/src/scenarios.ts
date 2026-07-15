export const ACCEPTANCE_IDS = [
  "AC-001",
  "AC-002",
  "AC-003",
  "AC-004",
  "AC-005",
  "AC-006",
  "AC-007",
  "AC-008",
  "AC-009",
  "AC-010",
  "AC-011",
  "AC-012",
  "AC-013",
  "AC-014",
  "AC-015",
  "AC-016",
  "AC-017",
  "AC-018",
  "AC-019",
] as const;

export type AcceptanceId = (typeof ACCEPTANCE_IDS)[number];
export type ScenarioPlatform = "darwin" | "linux" | "win32";
export type ScenarioInputFidelity = "real-input";

interface ScenarioDefinition {
  id: string;
  title: string;
  fixture: "ssh" | "ftp" | "s3" | null;
  acceptanceIds: readonly AcceptanceId[];
  platforms: readonly ScenarioPlatform[];
  inputFidelity: ScenarioInputFidelity;
  timeoutMs?: number;
}

export const ALL_PLATFORMS = ["darwin", "linux", "win32"] as const;
const UNIX_PLATFORMS = ["darwin", "linux"] as const;

export const SCENARIOS = [
  {
    id: "vault-lifecycle",
    title: "Create, lock, reject a wrong password, and unlock a real encrypted vault",
    fixture: null,
    acceptanceIds: ["AC-001"],
    platforms: ALL_PLATFORMS,
    inputFidelity: "real-input",
    timeoutMs: 180_000,
  },
  {
    id: "appearance-theme",
    title: "Apply and persist a readable light appearance",
    fixture: null,
    acceptanceIds: ["AC-018"],
    platforms: ALL_PLATFORMS,
    inputFidelity: "real-input",
    timeoutMs: 180_000,
  },
  {
    id: "ssh-terminal",
    title: "Verify unknown and changed host keys through a real SSH terminal",
    fixture: "ssh",
    acceptanceIds: ["AC-003"],
    platforms: UNIX_PLATFORMS,
    inputFidelity: "real-input",
    timeoutMs: 300_000,
  },
  {
    id: "session-lifecycle",
    title: "Keep SSH terminals, tunnels, and locked sessions isolated",
    fixture: "ssh",
    acceptanceIds: ["AC-005", "AC-013", "AC-014", "AC-016"],
    platforms: UNIX_PLATFORMS,
    inputFidelity: "real-input",
    timeoutMs: 600_000,
  },
  {
    id: "ftp-tab-isolation",
    title: "Keep two FTP tabs for one saved connection independent",
    fixture: "ftp",
    acceptanceIds: ["AC-004"],
    platforms: ALL_PLATFORMS,
    inputFidelity: "real-input",
    timeoutMs: 180_000,
  },
  {
    id: "ftp-recursive-transfer",
    title: "Upload and download a complete nested directory through the real FTP queue",
    fixture: "ftp",
    acceptanceIds: ["AC-006"],
    platforms: ALL_PLATFORMS,
    inputFidelity: "real-input",
    timeoutMs: 360_000,
  },
  {
    id: "transfer-resilience",
    title: "Resolve transfer conflicts and retry an interrupted FTP transfer",
    fixture: "ftp",
    acceptanceIds: ["AC-007", "AC-008"],
    platforms: ALL_PLATFORMS,
    inputFidelity: "real-input",
    timeoutMs: 600_000,
  },
  {
    id: "remote-edit-safety",
    title: "Edit through an external process and preserve the remote file on promotion failure",
    fixture: "ftp",
    acceptanceIds: ["AC-009"],
    platforms: ALL_PLATFORMS,
    inputFidelity: "real-input",
    timeoutMs: 300_000,
  },
  {
    id: "s3-buckets",
    title: "Create and browse buckets against a real S3-compatible server",
    fixture: "s3",
    acceptanceIds: ["AC-010"],
    platforms: ALL_PLATFORMS,
    inputFidelity: "real-input",
    timeoutMs: 180_000,
  },
  {
    id: "s3-sharing",
    title: "Choose upload ACLs, publish an object, and copy its public URL",
    fixture: "s3",
    acceptanceIds: ["AC-011", "AC-012"],
    platforms: ALL_PLATFORMS,
    inputFidelity: "real-input",
    timeoutMs: 300_000,
  },
  {
    id: "platform-shortcuts",
    title: "Use host shortcuts and selected-file actions through visible controls",
    fixture: "ftp",
    acceptanceIds: ["AC-017"],
    platforms: ALL_PLATFORMS,
    inputFidelity: "real-input",
    timeoutMs: 240_000,
  },
  {
    id: "directory-comparison",
    title: "Compare open local and remote folders without modifying either side",
    fixture: "ftp",
    acceptanceIds: ["AC-019"],
    platforms: ALL_PLATFORMS,
    inputFidelity: "real-input",
    timeoutMs: 180_000,
  },
] as const satisfies readonly ScenarioDefinition[];

export interface ManualNativeAcceptanceOwner {
  id: string;
  title: string;
  acceptanceIds: readonly AcceptanceId[];
  platforms: readonly ScenarioPlatform[];
  inputFidelity: "manual-native";
  reason: string;
}

export const MANUAL_NATIVE_ACCEPTANCE = [
  {
    id: "quick-unlock-native",
    title: "Touch ID and Windows Hello quick unlock",
    acceptanceIds: ["AC-002"],
    platforms: ["darwin", "win32"],
    inputFidelity: "manual-native",
    reason: "The biometric prompt is owned by the operating system and is outside WebDriver control.",
  },
  {
    id: "config-roundtrip-native",
    title: "Secret-free export and idempotent import through native pickers",
    acceptanceIds: ["AC-015"],
    platforms: ALL_PLATFORMS,
    inputFidelity: "manual-native",
    reason: "The export and import paths are selected through native operating-system dialogs.",
  },
] as const satisfies readonly ManualNativeAcceptanceOwner[];

export interface NotApplicableAcceptanceOwner {
  id: string;
  title: string;
  acceptanceIds: readonly AcceptanceId[];
  platforms: readonly ScenarioPlatform[];
  category: "not-applicable";
  reason: string;
}

export const NOT_APPLICABLE_ACCEPTANCE = [
  {
    id: "quick-unlock-linux-not-applicable",
    title: "Linux quick unlock",
    acceptanceIds: ["AC-002"],
    platforms: ["linux"],
    category: "not-applicable",
    reason: "Linux has no quick-unlock implementation.",
  },
] as const satisfies readonly NotApplicableAcceptanceOwner[];

export function validateManualNativeCatalog(
  catalog: readonly ManualNativeAcceptanceOwner[],
  automatedIds: readonly string[],
): string[] {
  const errors: string[] = [];
  const ids = new Set<string>();
  const automated = new Set(automatedIds);
  const reportedAutomatedConflicts = new Set<string>();

  for (const check of catalog) {
    if (automated.has(check.id) && !reportedAutomatedConflicts.has(check.id)) {
      errors.push(`${check.id}: conflicts with an automated scenario id`);
      reportedAutomatedConflicts.add(check.id);
    }
    if (check.title.trim().length === 0) errors.push(`${check.id}: title must not be empty`);
    if (check.platforms.length === 0) errors.push(`${check.id}: platforms must not be empty`);
    if (check.acceptanceIds.length === 0) {
      errors.push(`${check.id}: acceptanceIds must not be empty`);
    }
    if (check.reason.trim().length === 0) errors.push(`${check.id}: reason must not be empty`);
    if (ids.has(check.id)) errors.push(`${check.id}: duplicate manual-native id`);
    ids.add(check.id);
  }

  return errors;
}

export function validateScenarioCatalog(catalog: readonly ScenarioDefinition[]): string[] {
  const errors: string[] = [];
  const ids = new Set<string>();
  for (const scenario of catalog) {
    if (ids.has(scenario.id)) errors.push(`${scenario.id}: duplicate scenario id`);
    ids.add(scenario.id);
    if (scenario.platforms.length === 0) errors.push(`${scenario.id}: platforms must not be empty`);
    if (scenario.acceptanceIds.length === 0) {
      errors.push(`${scenario.id}: acceptanceIds must not be empty`);
    }
    if (new Set(scenario.platforms).size !== scenario.platforms.length) {
      errors.push(`${scenario.id}: platforms must not contain duplicates`);
    }
    if (new Set(scenario.acceptanceIds).size !== scenario.acceptanceIds.length) {
      errors.push(`${scenario.id}: acceptanceIds must not contain duplicates`);
    }
  }
  return errors;
}

export type Scenario = (typeof SCENARIOS)[number];
export type ScenarioId = Scenario["id"];

export const SCENARIO_IDS: readonly ScenarioId[] = Object.freeze(
  SCENARIOS.map(({ id }) => id),
);
