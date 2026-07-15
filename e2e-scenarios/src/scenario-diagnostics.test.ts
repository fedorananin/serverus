import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { settleWithin } from "./scenario-diagnostics";

describe("scenario diagnostics", () => {
  it("does not let screenshot success, failure, or a hang affect teardown", async () => {
    assert.equal(await settleWithin(Promise.resolve(), 100), true);
    assert.equal(await settleWithin(Promise.reject(new Error("capture failed")), 100), false);
    assert.equal(await settleWithin(new Promise(() => undefined), 10), false);
  });
});
