import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { SCENARIO_IDS } from "./scenarios";
import { resolveScenarioIds } from "./scenario-selection";

describe("resolveScenarioIds", () => {
  it("runs the complete catalog by default", () => {
    assert.deepEqual(resolveScenarioIds(SCENARIO_IDS, {}), SCENARIO_IDS);
  });

  it("filters requested scenarios while preserving catalog order", () => {
    assert.deepEqual(
      resolveScenarioIds(SCENARIO_IDS, {
        E2E_SCENARIOS: "s3-buckets, vault-lifecycle, s3-buckets",
      }),
      ["vault-lifecycle", "s3-buckets"],
    );
  });

  it("fails fast for an unknown scenario", () => {
    assert.throws(
      () => resolveScenarioIds(SCENARIO_IDS, { E2E_SCENARIOS: "not-real" }),
      /Unknown E2E scenario "not-real".*vault-lifecycle/s,
    );
  });

  it("applies numeric sharding after explicit filtering", () => {
    const env = {
      E2E_SCENARIOS: "vault-lifecycle,ssh-terminal,s3-buckets",
      E2E_SCENARIO_SHARDS_TOTAL: "2",
    };

    assert.deepEqual(resolveScenarioIds(SCENARIO_IDS, { ...env, E2E_SCENARIO_SHARD_INDEX: "1" }), [
      "vault-lifecycle",
      "ssh-terminal",
    ]);
    assert.deepEqual(resolveScenarioIds(SCENARIO_IDS, { ...env, E2E_SCENARIO_SHARD_INDEX: "2" }), [
      "s3-buckets",
    ]);
  });

  it("requires both sharding variables", () => {
    assert.throws(
      () =>
        resolveScenarioIds(SCENARIO_IDS, {
          E2E_SCENARIO_SHARD_INDEX: "1",
        }),
      /requires both E2E_SCENARIO_SHARD_INDEX and E2E_SCENARIO_SHARDS_TOTAL/,
    );
  });

  it("rejects an out-of-range or empty shard", () => {
    assert.throws(
      () =>
        resolveScenarioIds(["one"], {
          E2E_SCENARIO_SHARD_INDEX: "2",
          E2E_SCENARIO_SHARDS_TOTAL: "2",
        }),
      /selects no scenarios/,
    );
  });
});
