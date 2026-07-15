import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, it } from "node:test";

import { validateScenarioLayout } from "./scenario-layout";

const temporaryDirectories: string[] = [];

function scenarioRoot(): string {
  const workspace = mkdtempSync(join(tmpdir(), "serverus-scenarios-"));
  const root = join(workspace, "scenarios");
  mkdirSync(root);
  temporaryDirectories.push(workspace);
  return root;
}

function addScenario(
  root: string,
  id: string,
  source = `describe("@${id}", () => { it("runs", () => {}); });`,
): void {
  const directory = join(root, id);
  mkdirSync(directory, { recursive: true });
  writeFileSync(join(directory, `${id}.e2e.spec.ts`), source);
}

function addScenarioSupport(root: string, id: string, source: string): void {
  const directory = join(root, id);
  mkdirSync(directory, { recursive: true });
  writeFileSync(join(directory, `${id}.support.ts`), source);
}

function addSharedSupport(root: string, name: string, source: string): void {
  const directory = join(root, "..", "support");
  mkdirSync(directory, { recursive: true });
  writeFileSync(join(directory, `${name}.ts`), source);
}

afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) {
    rmSync(directory, { recursive: true, force: true });
  }
});

describe("validateScenarioLayout", () => {
  it("accepts a one-to-one catalog, directory, entry file, and tag", () => {
    const root = scenarioRoot();
    addScenario(root, "vault-lifecycle");

    assert.deepEqual(validateScenarioLayout(root, ["vault-lifecycle"]), []);
  });

  it("reports missing entries, unregistered directories, and missing tags together", () => {
    const root = scenarioRoot();
    mkdirSync(join(root, "vault-lifecycle"));
    addScenario(
      root,
      "ftp-recursive-transfer",
      'describe("FTP transfer", () => { it("runs", () => {}); });',
    );

    assert.deepEqual(validateScenarioLayout(root, ["vault-lifecycle"]), [
      "vault-lifecycle: missing vault-lifecycle.e2e.spec.ts",
      "ftp-recursive-transfer: scenario directory is not registered",
    ]);
  });

  it("requires the registered scenario tag in its entry spec", () => {
    const root = scenarioRoot();
    addScenario(
      root,
      "vault-lifecycle",
      'describe("Vault lifecycle", () => { it("runs", () => {}); });',
    );

    assert.deepEqual(validateScenarioLayout(root, ["vault-lifecycle"]), [
      'vault-lifecycle: entry spec must declare describe("@vault-lifecycle", ...)',
    ]);
  });

  it("rejects comment-only tags and suites without executable tests", () => {
    const root = scenarioRoot();
    addScenario(root, "comment-only", "// @comment-only\n");
    addScenario(root, "empty-suite", 'describe("@empty-suite", () => {});');

    assert.deepEqual(validateScenarioLayout(root, ["comment-only", "empty-suite"]), [
      "comment-only: entry spec must declare describe(\"@comment-only\", ...)",
      "empty-suite: entry spec must contain at least one direct executable test",
    ]);
  });

  it("requires the executable test to belong to the tagged scenario suite", () => {
    const root = scenarioRoot();
    addScenario(
      root,
      "detached-test",
      'describe("@detached-test", () => {}); describe("helper", () => { it("runs", () => {}); });',
    );

    assert.deepEqual(validateScenarioLayout(root, ["detached-test"]), [
      "detached-test: entry spec must contain at least one direct executable test",
    ]);
  });

  it("rejects unreachable, helper-nested, and additional entry specs", () => {
    const root = scenarioRoot();
    addScenario(
      root,
      "hidden-test",
      `describe("@hidden-test", () => {
        if (false) it("never runs", () => {});
        function helper() { it("nested", () => {}); }
      });`,
    );
    writeFileSync(
      join(root, "hidden-test", "second.e2e.spec.ts"),
      'describe("@hidden-test", () => { it("extra", () => {}); });',
    );

    assert.deepEqual(validateScenarioLayout(root, ["hidden-test"]), [
      "hidden-test: entry spec must contain at least one direct executable test",
      "hidden-test: unexpected additional entry spec second.e2e.spec.ts",
    ]);
  });

  it("rejects nested and root-level specs that WDIO would never select", () => {
    const root = scenarioRoot();
    addScenario(root, "vault-lifecycle");
    mkdirSync(join(root, "vault-lifecycle", "nested"));
    writeFileSync(
      join(root, "vault-lifecycle", "nested", "hidden.e2e.spec.ts"),
      'describe("@vault-lifecycle", () => { it("hidden", () => {}); });',
    );
    writeFileSync(
      join(root, "orphan.e2e.spec.ts"),
      'describe("@orphan", () => { it("hidden", () => {}); });',
    );

    assert.deepEqual(validateScenarioLayout(root, ["vault-lifecycle"]), [
      "orphan.e2e.spec.ts: entry spec is outside a registered scenario directory",
      "vault-lifecycle: unexpected additional entry spec nested/hidden.e2e.spec.ts",
    ]);
  });

  it("rejects browser script execution in a real-input scenario spec", () => {
    const root = scenarioRoot();
    addScenario(
      root,
      "vault-lifecycle",
      `describe("@vault-lifecycle", () => {
        it("runs", async () => {
          await browser.execute(() => document.title);
          await browser["executeAsync"](() => undefined);
        });
      });`,
    );

    assert.deepEqual(validateScenarioLayout(root, ["vault-lifecycle"]), [
      "vault-lifecycle/vault-lifecycle.e2e.spec.ts:3: browser.execute is forbidden in real-input scenarios",
      "vault-lifecycle/vault-lifecycle.e2e.spec.ts:4: browser.executeAsync is forbidden in real-input scenarios",
    ]);
  });

  it("rejects browser execution through aliases and globalThis", () => {
    const root = scenarioRoot();
    addScenario(
      root,
      "vault-lifecycle",
      `describe("@vault-lifecycle", () => {
        it("runs", async () => {
          const driver = browser;
          await driver.execute(() => document.title);
          await globalThis.browser.executeAsync(() => undefined);
          const run = browser.execute;
          await run(() => document.title);
        });
      });`,
    );

    assert.deepEqual(validateScenarioLayout(root, ["vault-lifecycle"]), [
      "vault-lifecycle/vault-lifecycle.e2e.spec.ts:4: execute() is forbidden in real-input scenarios",
      "vault-lifecycle/vault-lifecycle.e2e.spec.ts:5: executeAsync() is forbidden in real-input scenarios",
      "vault-lifecycle/vault-lifecycle.e2e.spec.ts:6: execute reference is forbidden in real-input scenarios",
    ]);
  });

  it("rejects in-spec Mocha retry overrides", () => {
    const root = scenarioRoot();
    addScenario(
      root,
      "vault-lifecycle",
      `describe("@vault-lifecycle", function () {
        this.retries(2);
        it("runs", async function () { this.retries(1); });
      });`,
    );

    assert.deepEqual(validateScenarioLayout(root, ["vault-lifecycle"]), [
      "vault-lifecycle/vault-lifecycle.e2e.spec.ts:2: retries() is forbidden in real-input scenarios",
      "vault-lifecycle/vault-lifecycle.e2e.spec.ts:3: retries() is forbidden in real-input scenarios",
    ]);
  });

  it("rejects DOM event, form, and click shims in scenario support", () => {
    const root = scenarioRoot();
    addScenario(root, "vault-lifecycle");
    addScenarioSupport(
      root,
      "vault-lifecycle",
      `const button = document.querySelector("button") as HTMLButtonElement;
      button.dispatchEvent(new MouseEvent("contextmenu"));
      document.querySelector("form")?.requestSubmit();
      button.click();`,
    );

    assert.deepEqual(validateScenarioLayout(root, ["vault-lifecycle"]), [
      "vault-lifecycle/vault-lifecycle.support.ts:2: dispatchEvent is forbidden in real-input scenarios",
      "vault-lifecycle/vault-lifecycle.support.ts:3: requestSubmit is forbidden in real-input scenarios",
      "vault-lifecycle/vault-lifecycle.support.ts:4: DOM click() is forbidden in real-input scenarios",
    ]);
  });

  it("rejects direct value assignment in shared scenario support", () => {
    const root = scenarioRoot();
    addScenario(root, "vault-lifecycle");
    addSharedSupport(
      root,
      "forms",
      `const select = document.querySelector("select") as HTMLSelectElement;
      select.value = "password";
      select["value"] += "-fallback";`,
    );

    assert.deepEqual(validateScenarioLayout(root, ["vault-lifecycle"]), [
      "support/forms.ts:2: direct .value assignment is forbidden in real-input scenarios",
      "support/forms.ts:3: direct .value assignment is forbidden in real-input scenarios",
    ]);
  });

  it("scans every TypeScript helper in a registered scenario directory", () => {
    const root = scenarioRoot();
    addScenario(root, "vault-lifecycle");
    writeFileSync(
      join(root, "vault-lifecycle", "interaction-helper.ts"),
      'document.body.dispatchEvent(new MouseEvent("contextmenu"));',
    );

    assert.deepEqual(validateScenarioLayout(root, ["vault-lifecycle"]), [
      "vault-lifecycle/interaction-helper.ts:1: dispatchEvent is forbidden in real-input scenarios",
    ]);
  });

  it("allows WebdriverIO click and value APIs", () => {
    const root = scenarioRoot();
    addScenario(root, "vault-lifecycle");
    addSharedSupport(
      root,
      "webdriver",
      `export async function choose(element: WebdriverIO.Element): Promise<void> {
        await element.click();
        await element.setValue("password");
      }`,
    );

    assert.deepEqual(validateScenarioLayout(root, ["vault-lifecycle"]), []);
  });

  it("does not confuse a WebdriverIO element with a DOM binding from another scope", () => {
    const root = scenarioRoot();
    addScenario(root, "vault-lifecycle");
    addSharedSupport(
      root,
      "scoped-webdriver",
      `function dispatchNative(): void {
        const element = document.querySelector("button") as HTMLButtonElement;
        element.focus();
      }
      export async function choose(element: WebdriverIO.Element): Promise<void> {
        await element.click();
      }`,
    );

    assert.deepEqual(validateScenarioLayout(root, ["vault-lifecycle"]), []);
  });
});
