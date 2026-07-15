import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  DEFAULT_SCENARIO_TIMEOUT_MS,
  scenarioRunnerTimeoutMs,
  scenarioTimeoutMs,
} from "./scenario-timeout";

describe("scenario timeouts", () => {
  it("uses the tagged suite's focused catalog budget", () => {
    const catalog = [
      { id: "short", timeoutMs: 10_000 },
      { id: "default" },
      { id: "long", timeoutMs: 600_000 },
    ];

    assert.equal(scenarioTimeoutMs("@short", catalog), 10_000);
    assert.equal(scenarioTimeoutMs("@default", catalog), DEFAULT_SCENARIO_TIMEOUT_MS);
    assert.equal(scenarioTimeoutMs("@long", catalog), 600_000);
    assert.throws(() => scenarioTimeoutMs("@missing", catalog), /Cannot resolve/u);
    assert.throws(() => scenarioTimeoutMs("short", catalog), /Cannot resolve/u);
  });

  it("bounds the WDIO process above the sum of selected test budgets", () => {
    assert.equal(scenarioRunnerTimeoutMs([{ id: "one", timeoutMs: 10_000 }]), 900_000);
    assert.equal(
      scenarioRunnerTimeoutMs([
        { id: "one", timeoutMs: 600_000 },
        { id: "two", timeoutMs: 600_000 },
      ]),
      1_800_000,
    );
  });
});
