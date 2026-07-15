import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it } from "node:test";

import { validateScenarioLayout } from "./scenario-layout";

describe("scenario right-button input gate", () => {
  it("rejects automation unsupported by the pinned embedded driver", () => {
    const workspace = mkdtempSync(join(tmpdir(), "serverus-right-button-"));
    const root = join(workspace, "scenarios");
    const scenario = join(root, "vault-lifecycle");
    mkdirSync(scenario, { recursive: true });
    writeFileSync(
      join(scenario, "vault-lifecycle.e2e.spec.ts"),
      `describe("@vault-lifecycle", () => {
        it("runs", async () => {
          await option.click({ button: "right" });
          await browser.action("pointer").down({ button: 2 }).perform();
        });
      });`,
    );

    try {
      assert.deepEqual(validateScenarioLayout(root, ["vault-lifecycle"]), [
        "vault-lifecycle/vault-lifecycle.e2e.spec.ts:3: right-button automation is forbidden in real-input scenarios",
        "vault-lifecycle/vault-lifecycle.e2e.spec.ts:4: right-button automation is forbidden in real-input scenarios",
      ]);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
