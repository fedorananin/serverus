import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { scenarioBuildEnvironment } from "./build-environment";

describe("scenario build environment", () => {
  it("defaults Cargo to one compiler job for memory-constrained runners", () => {
    assert.equal(
      scenarioBuildEnvironment({ PATH: "/bin" }, "/tmp/scenarios").CARGO_BUILD_JOBS,
      "1",
    );
  });

  it("honours an explicit Cargo setting before the scenario override", () => {
    assert.equal(
      scenarioBuildEnvironment(
        { CARGO_BUILD_JOBS: "4", SERVERUS_SCENARIO_BUILD_JOBS: "1" },
        "/tmp/scenarios",
      ).CARGO_BUILD_JOBS,
      "4",
    );
    assert.equal(
      scenarioBuildEnvironment({ SERVERUS_SCENARIO_BUILD_JOBS: "1" }, "/tmp/scenarios")
        .CARGO_BUILD_JOBS,
      "1",
    );
  });

  it("rejects invalid scenario job limits", () => {
    assert.throws(
      () =>
        scenarioBuildEnvironment({ SERVERUS_SCENARIO_BUILD_JOBS: "0" }, "/tmp/scenarios"),
      /positive integer/,
    );
  });
});
