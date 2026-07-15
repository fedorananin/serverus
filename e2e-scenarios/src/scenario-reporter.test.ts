import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { scenarioResultFromSuite } from "./scenario-reporter";

describe("secret-free scenario reporter", () => {
  it("reports one passed result for a fully passing tagged suite", () => {
    assert.deepEqual(
      scenarioResultFromSuite({
        title: "@vault-lifecycle",
        duration: 12.6,
        hooks: [{ state: "passed" }],
        tests: [{ state: "passed" }, { state: "passed" }],
      }),
      { scenarioId: "vault-lifecycle", status: "passed", durationMs: 13 },
    );
  });

  it("reports a declared all-skipped suite without copying skip reasons", () => {
    assert.deepEqual(
      scenarioResultFromSuite({
        title: "@ssh-terminal",
        duration: 0,
        hooks: [],
        tests: [{ state: "skipped", pendingReason: "may contain environment details" }],
      }),
      { scenarioId: "ssh-terminal", status: "skipped", durationMs: 0 },
    );
  });

  it("treats the pending state emitted by a Mocha this.skip as skipped", () => {
    assert.deepEqual(
      scenarioResultFromSuite({
        title: "@ssh-terminal",
        duration: 1,
        hooks: [],
        tests: [{ state: "pending", pendingReason: "sync skip; aborting execution" }],
      }),
      { scenarioId: "ssh-terminal", status: "skipped", durationMs: 1 },
    );
  });

  it("includes nested tests and hooks when accounting for a tagged suite", () => {
    assert.equal(
      scenarioResultFromSuite({
        title: "@nested-skip",
        duration: 5,
        hooks: [],
        tests: [{ state: "passed" }],
        suites: [
          {
            title: "platform branch",
            duration: 1,
            hooks: [],
            tests: [{ state: "skipped", pendingReason: "unexpected platform state" }],
          },
        ],
      })?.status,
      "failed",
    );
    assert.equal(
      scenarioResultFromSuite({
        title: "@nested-hook",
        duration: 5,
        hooks: [],
        tests: [],
        suites: [
          {
            title: "runtime setup",
            duration: 1,
            hooks: [{ state: "failed", error: new Error("must not be serialized") }],
            tests: [{ state: "passed" }],
          },
        ],
      })?.status,
      "failed",
    );
  });

  it("fails partial skips, hook failures, empty suites, and ignores untagged suites", () => {
    assert.equal(
      scenarioResultFromSuite({
        title: "@mixed",
        duration: 2,
        hooks: [],
        tests: [{ state: "passed" }, { state: "skipped" }],
      })?.status,
      "failed",
    );
    assert.equal(
      scenarioResultFromSuite({
        title: "@hook-failure",
        duration: 2,
        hooks: [{ state: "failed", error: new Error("must not be serialized") }],
        tests: [{ state: "passed" }],
      })?.status,
      "failed",
    );
    assert.equal(
      scenarioResultFromSuite({ title: "@empty", duration: 0, hooks: [], tests: [] })
        ?.status,
      "failed",
    );
    assert.equal(
      scenarioResultFromSuite({
        title: "helper suite",
        duration: 1,
        hooks: [],
        tests: [{ state: "passed" }],
      }),
      null,
    );
  });
});
