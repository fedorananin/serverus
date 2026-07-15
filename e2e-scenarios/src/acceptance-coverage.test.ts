import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  parseDocumentedAcceptanceIds,
  summarizeAcceptanceCoverage,
  summarizePlatformAcceptanceCoverage,
  validateAcceptanceCoverage,
  validateAcceptanceDocument,
  validatePlatformAcceptanceCoverage,
} from "./acceptance-coverage";

describe("validateAcceptanceCoverage", () => {
  it("reports automated and manual-native coverage separately", () => {
    assert.deepEqual(
      summarizeAcceptanceCoverage(
        ["AC-001", "AC-002", "AC-003"],
        [{ id: "automated", acceptanceIds: ["AC-001", "AC-003"] }],
        [{ id: "manual", acceptanceIds: ["AC-002"] }],
      ),
      { automated: ["AC-001", "AC-003"], manualNative: ["AC-002"] },
    );
  });

  it("reports per-platform automation and expected skips without hiding either", () => {
    assert.deepEqual(
      summarizePlatformAcceptanceCoverage(
        ["AC-001", "AC-002", "AC-003"],
        [
          { id: "all", acceptanceIds: ["AC-001"], platforms: ["darwin", "win32"] },
          { id: "unix", acceptanceIds: ["AC-003"], platforms: ["darwin"] },
        ],
        [{ id: "native", acceptanceIds: ["AC-002"], platforms: ["darwin", "win32"] }],
        [{ id: "linux-na", acceptanceIds: ["AC-002"], platforms: ["linux"] }],
        "win32",
      ),
      {
        automated: ["AC-001"],
        expectedSkipped: ["AC-003"],
        manualNative: ["AC-002"],
        notApplicable: [],
      },
    );
  });

  it("reports explicit not-applicable coverage instead of hiding a platform gap", () => {
    assert.deepEqual(
      summarizePlatformAcceptanceCoverage(
        ["AC-001", "AC-002"],
        [{ id: "vault", acceptanceIds: ["AC-001"], platforms: ["darwin", "linux"] }],
        [{ id: "quick-unlock", acceptanceIds: ["AC-002"], platforms: ["darwin"] }],
        [{ id: "quick-unlock-linux-na", acceptanceIds: ["AC-002"], platforms: ["linux"] }],
        "linux",
      ),
      {
        automated: ["AC-001"],
        expectedSkipped: [],
        manualNative: [],
        notApplicable: ["AC-002"],
      },
    );
  });

  it("requires exactly one accounting category for every criterion on every platform", () => {
    assert.deepEqual(
      validatePlatformAcceptanceCoverage(
        ["AC-001", "AC-002", "AC-003", "AC-004"],
        [
          { id: "vault", acceptanceIds: ["AC-001"], platforms: ["darwin", "linux"] },
          { id: "ssh", acceptanceIds: ["AC-003"], platforms: ["darwin"] },
        ],
        [
          { id: "native", acceptanceIds: ["AC-002"], platforms: ["darwin"] },
          { id: "overlap", acceptanceIds: ["AC-003"], platforms: ["linux"] },
        ],
        [
          { id: "linux-na", acceptanceIds: ["AC-002"], platforms: ["linux"] },
          { id: "unknown-na", acceptanceIds: ["AC-999"], platforms: ["linux"] },
        ],
        ["darwin", "linux"],
      ),
      [
        "darwin/AC-004: acceptance criterion has no platform category",
        "linux/AC-003: expected exactly one platform category, found expected-skip (ssh), manual-native (overlap)",
        "linux/AC-004: acceptance criterion has no platform category",
        "unknown-na: references unknown acceptance criterion AC-999",
      ],
    );
  });

  it("extracts acceptance headings and rejects drift from the typed catalog", () => {
    const documented = parseDocumentedAcceptanceIds(
      "# Acceptance\n\n## AC-001: First\n\ntext\n\n## AC-002: Second\n",
    );
    assert.deepEqual(documented, ["AC-001", "AC-002"]);
    assert.deepEqual(validateAcceptanceDocument(["AC-001", "AC-003"], documented), [
      "AC-002: documented acceptance criterion is missing from the typed catalog",
      "AC-003: typed acceptance criterion is missing from the acceptance document",
    ]);
  });

  it("accepts exactly one owner for every acceptance criterion", () => {
    assert.deepEqual(
      validateAcceptanceCoverage(
        ["AC-001", "AC-002"],
        [{ id: "vault", acceptanceIds: ["AC-001"] }],
        [{ id: "quick-unlock-manual", acceptanceIds: ["AC-002"] }],
      ),
      [],
    );
  });

  it("reports missing, duplicate, and unknown acceptance ownership", () => {
    assert.deepEqual(
      validateAcceptanceCoverage(
        ["AC-001", "AC-002", "AC-003"],
        [
          { id: "vault", acceptanceIds: ["AC-001", "AC-999"] },
          { id: "second-vault", acceptanceIds: ["AC-001"] },
        ],
        [{ id: "manual-vault", acceptanceIds: ["AC-001"] }],
      ),
      [
        "AC-001: expected exactly one owner, found vault, second-vault, manual-vault",
        "AC-002: acceptance criterion has no scenario or manual-native owner",
        "AC-003: acceptance criterion has no scenario or manual-native owner",
        "vault: references unknown acceptance criterion AC-999",
      ],
    );
  });
});
