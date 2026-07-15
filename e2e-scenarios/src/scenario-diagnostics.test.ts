import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { settleWithin, shouldCaptureFailureDiagnostic } from "./scenario-diagnostics";

describe("scenario diagnostics", () => {
  it("does not let screenshot success, failure, or a hang affect teardown", async () => {
    assert.equal(await settleWithin(Promise.resolve(), 100), true);
    assert.equal(await settleWithin(Promise.reject(new Error("capture failed")), 100), false);
    assert.equal(await settleWithin(new Promise(() => undefined), 10), false);
  });

  it("requests a screenshot only for an actual failure", () => {
    assert.equal(shouldCaptureFailureDiagnostic({ passed: false, skipped: true }), false);
    assert.equal(shouldCaptureFailureDiagnostic({ passed: true, skipped: false }), false);
    assert.equal(shouldCaptureFailureDiagnostic({ passed: false, skipped: false }), true);
  });
});
