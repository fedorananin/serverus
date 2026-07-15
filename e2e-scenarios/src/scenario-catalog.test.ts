import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  validateAcceptanceCoverage,
  validatePlatformAcceptanceCoverage,
} from "./acceptance-coverage";
import {
  ACCEPTANCE_IDS,
  ALL_PLATFORMS,
  MANUAL_NATIVE_ACCEPTANCE,
  NOT_APPLICABLE_ACCEPTANCE,
  SCENARIOS,
  validateManualNativeCatalog,
  validateScenarioCatalog,
} from "./scenarios";
import {
  MANUAL_NATIVE_SUPPLEMENTS,
  validateManualNativeSupplements,
} from "./scenario-supplements";

describe("scenario catalog", () => {
  it("assigns every acceptance criterion to exactly one explicit owner", () => {
    assert.deepEqual(
      validateAcceptanceCoverage(ACCEPTANCE_IDS, SCENARIOS, MANUAL_NATIVE_ACCEPTANCE),
      [],
    );
  });

  it("accounts for every acceptance criterion exactly once on every platform", () => {
    assert.deepEqual(
      validatePlatformAcceptanceCoverage(
        ACCEPTANCE_IDS,
        SCENARIOS,
        MANUAL_NATIVE_ACCEPTANCE,
        NOT_APPLICABLE_ACCEPTANCE,
        ALL_PLATFORMS,
      ),
      [],
    );
  });

  it("requires executable scenarios to declare platforms, acceptance IDs, and input fidelity", () => {
    assert.deepEqual(validateScenarioCatalog(SCENARIOS), []);
  });

  it("owns AC-004 with the all-platform FTP tab-isolation scenario", () => {
    const owners = SCENARIOS.filter(({ acceptanceIds }) =>
      acceptanceIds.some((id) => id === "AC-004"),
    );
    assert.deepEqual(
      owners.map(({ id, fixture, platforms, inputFidelity }) => ({
        id,
        fixture,
        platforms: [...platforms],
        inputFidelity,
      })),
      [
        {
          id: "ftp-tab-isolation",
          fixture: "ftp",
          platforms: [...ALL_PLATFORMS],
          inputFidelity: "real-input",
        },
      ],
    );
  });

  it("requires actionable, uniquely named manual-native owners", () => {
    assert.deepEqual(
      validateManualNativeCatalog(MANUAL_NATIVE_ACCEPTANCE, SCENARIOS.map(({ id }) => id)),
      [],
    );
    assert.deepEqual(
      validateManualNativeCatalog(
        [
          {
            id: "vault-lifecycle",
            title: "",
            acceptanceIds: [],
            platforms: [],
            inputFidelity: "manual-native",
            reason: "",
          },
          {
            id: "vault-lifecycle",
            title: "Duplicate",
            acceptanceIds: ["AC-002"],
            platforms: ["darwin"],
            inputFidelity: "manual-native",
            reason: "Native prompt",
          },
        ],
        ["vault-lifecycle"],
      ),
      [
        "vault-lifecycle: conflicts with an automated scenario id",
        "vault-lifecycle: title must not be empty",
        "vault-lifecycle: platforms must not be empty",
        "vault-lifecycle: acceptanceIds must not be empty",
        "vault-lifecycle: reason must not be empty",
        "vault-lifecycle: duplicate manual-native id",
      ],
    );
  });

  it("keeps manual-native supplements attached to an automated owner", () => {
    assert.deepEqual(
      validateManualNativeSupplements(
        MANUAL_NATIVE_SUPPLEMENTS,
        SCENARIOS,
        [...SCENARIOS, ...MANUAL_NATIVE_ACCEPTANCE].map(({ id }) => id),
      ),
      [],
    );
    assert.equal(
      SCENARIOS.filter(({ acceptanceIds }) =>
        acceptanceIds.some((id) => id === MANUAL_NATIVE_SUPPLEMENTS[0].acceptanceId),
      ).length,
      1,
      "a supplement must not become a second acceptance owner",
    );
  });

  it("keeps native right-click coverage manual on every automated platform", () => {
    const supplement = MANUAL_NATIVE_SUPPLEMENTS.find(
      ({ id }) => id === "platform-context-menu-native",
    );

    assert.ok(supplement);
    assert.deepEqual(supplement.platforms, ALL_PLATFORMS);
    assert.match(supplement.reason, /tauri-plugin-wdio-webdriver 1\.2\.0/);
  });

  it("keeps Shift+F10 coverage manual while the embedded driver drops modifiers", () => {
    const supplement = MANUAL_NATIVE_SUPPLEMENTS.find(
      ({ id }) => id === "platform-keyboard-context-menu-native",
    );

    assert.ok(supplement);
    assert.deepEqual(supplement.platforms, ALL_PLATFORMS);
    assert.equal(supplement.coverageImpact, "required-path");
    assert.match(supplement.manualAction, /Shift\+F10/);
    assert.match(supplement.reason, /modifier/i);
  });

  it("rejects incomplete or detached manual-native supplements", () => {
    assert.deepEqual(
      validateManualNativeSupplements(
        [
          {
            id: "platform-shortcuts",
            title: "",
            acceptanceId: "AC-001",
            automatedOwnerId: "platform-shortcuts",
            platforms: [],
            inputFidelity: "manual-native",
            coverageImpact: "invalid" as never,
            manualAction: "",
            reason: "",
          },
        ],
        SCENARIOS,
        SCENARIOS.map(({ id }) => id),
      ),
      [
        "platform-shortcuts: conflicts with an acceptance owner id",
        "platform-shortcuts: title must not be empty",
        "platform-shortcuts: platforms must not be empty",
        "platform-shortcuts: coverageImpact must be required-path or additional-variant",
        "platform-shortcuts: AC-001 is not owned by automated scenario platform-shortcuts",
        "platform-shortcuts: manualAction must not be empty",
        "platform-shortcuts: reason must not be empty",
      ],
    );
  });

  it("rejects duplicate supplements and platforms outside the automated owner", () => {
    const supplement = {
      id: "arrow-transfer",
      title: "Arrow transfer",
      acceptanceId: "AC-017" as const,
      automatedOwnerId: "shortcut-owner",
      platforms: ["darwin", "darwin"] as const,
      inputFidelity: "manual-native" as const,
      coverageImpact: "required-path" as const,
      manualAction: "Press the native shortcut.",
      reason: "The driver cannot preserve the modifier.",
    };
    assert.deepEqual(
      validateManualNativeSupplements(
        [supplement, { ...supplement, platforms: ["win32"] }],
        [{ id: "shortcut-owner", acceptanceIds: ["AC-017"], platforms: ["darwin"] }],
        [],
      ),
      [
        "arrow-transfer: platforms must not contain duplicates",
        "arrow-transfer: duplicate manual-native supplement id",
        "arrow-transfer: platform win32 is not automated by shortcut-owner",
      ],
    );
  });
});
