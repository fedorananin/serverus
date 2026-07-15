import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, it } from "node:test";
import ts from "typescript";

const source = ts.createSourceFile(
  "wdio.scenarios.conf.ts",
  readFileSync(resolve("wdio.scenarios.conf.ts"), "utf8"),
  ts.ScriptTarget.Latest,
  true,
);

function propertyName(node: ts.PropertyName): string | null {
  return ts.isIdentifier(node) || ts.isStringLiteralLike(node) ? node.text : null;
}

function properties(name: string): ts.PropertyAssignment[] {
  const matches: ts.PropertyAssignment[] = [];
  function visit(node: ts.Node): void {
    if (ts.isPropertyAssignment(node) && propertyName(node.name) === name) matches.push(node);
    ts.forEachChild(node, visit);
  }
  visit(source);
  return matches;
}

function numericValue(property: ts.PropertyAssignment): number | null {
  return ts.isNumericLiteral(property.initializer) ? Number(property.initializer.text) : null;
}

describe("scenario WebDriver safety policy", () => {
  it("keeps every automatic retry mechanism disabled", () => {
    assert.deepEqual(properties("connectionRetryCount").map(numericValue), [0]);
    assert.deepEqual(properties("specFileRetries").map(numericValue), [0]);
    assert.deepEqual(properties("retries").map(numericValue), [0]);
  });

  it("keeps command logging silent and Tauri application log capture disabled", () => {
    const levels = properties("logLevel").map(({ initializer }) =>
      ts.isStringLiteralLike(initializer) ? initializer.text : null,
    );
    assert.deepEqual(levels, ["error", "silent"]);
    for (const name of ["captureBackendLogs", "captureFrontendLogs"]) {
      assert.deepEqual(
        properties(name).map(({ initializer }) => initializer.kind === ts.SyntaxKind.FalseKeyword),
        [true],
      );
    }
  });
});
