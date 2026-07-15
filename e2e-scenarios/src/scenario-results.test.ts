import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { parseScenarioResults, validateScenarioResults } from "./scenario-results";

const selected = [
  { id: "vault", platforms: ["darwin", "linux", "win32"] },
  { id: "ssh", platforms: ["darwin", "linux"] },
] as const;

describe("scenario result accounting", () => {
  it("accepts passed supported scenarios and the declared Windows skip", () => {
    assert.deepEqual(
      validateScenarioResults(selected, "win32", [
        { scenarioId: "vault", status: "passed", durationMs: 10 },
        { scenarioId: "ssh", status: "skipped", durationMs: 0 },
      ]),
      [],
    );
  });

  it("rejects an undeclared skip on a supported platform", () => {
    assert.deepEqual(
      validateScenarioResults(selected, "linux", [
        { scenarioId: "vault", status: "skipped", durationMs: 1 },
        { scenarioId: "ssh", status: "passed", durationMs: 2 },
      ]),
      ["vault: expected passed, got skipped"],
    );
  });

  it("rejects unexpected skips, missing results, duplicates, and unknown scenarios", () => {
    assert.deepEqual(
      validateScenarioResults(selected, "linux", [
        { scenarioId: "vault", status: "skipped", durationMs: 1 },
        { scenarioId: "vault", status: "passed", durationMs: 2 },
        { scenarioId: "other", status: "passed", durationMs: 3 },
      ]),
      [
        "vault: expected one result, found 2",
        "ssh: expected one result, found 0",
        "other: reporter emitted an unselected scenario",
      ],
    );
  });

  it("parses only the secret-free result schema", () => {
    assert.deepEqual(
      parseScenarioResults(
        '{"scenarioId":"vault","status":"passed","durationMs":12}\n' +
          '{"scenarioId":"ssh","status":"skipped","durationMs":0}\n',
      ),
      [
        { scenarioId: "vault", status: "passed", durationMs: 12 },
        { scenarioId: "ssh", status: "skipped", durationMs: 0 },
      ],
    );
    assert.throws(
      () =>
        parseScenarioResults(
          '{"scenarioId":"vault","status":"failed","durationMs":1,"error":"secret"}\n',
        ),
      /unexpected field error/,
    );
  });
});
