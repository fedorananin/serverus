import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, it } from "node:test";

import { validateScenarioLayout } from "./scenario-layout";

function scenarioRoot(source: string): { root: string; workspace: string } {
  const workspace = mkdtempSync(join(tmpdir(), "serverus-keyboard-action-"));
  const root = join(workspace, "scenarios");
  const scenario = join(root, "platform-shortcuts");
  mkdirSync(scenario, { recursive: true });
  writeFileSync(join(scenario, "platform-shortcuts.e2e.spec.ts"), source);
  return { root, workspace };
}

describe("scenario keyboard action gate", () => {
  it("rejects raw action sequences instead of guessing their held-key state", () => {
    const { root, workspace } = scenarioRoot(`describe("@platform-shortcuts", () => {
      it("runs", async () => {
        await browser.action("key").down(Key.Shift).down(Key.F10).perform();
        const action = browser.action("key"); action.down(Key.Shift); action.down(Key["F10"]);
        function pressShortcut(key) { return browser.action("key").down(primaryModifier).down(key).perform(); }
        await pressShortcut(Key.F10);
      });
    });`);

    try {
      assert.deepEqual(validateScenarioLayout(root, ["platform-shortcuts"]), [
        "platform-shortcuts/platform-shortcuts.e2e.spec.ts:3: raw keyboard action() is forbidden in real-input scenarios",
        "platform-shortcuts/platform-shortcuts.e2e.spec.ts:4: raw keyboard action() is forbidden in real-input scenarios",
        "platform-shortcuts/platform-shortcuts.e2e.spec.ts:5: raw keyboard action() is forbidden in real-input scenarios",
      ]);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it("allows the one runtime-guarded primary-shortcut helper", () => {
    const { root, workspace } = scenarioRoot(`describe("@platform-shortcuts", () => {
      it("runs", async () => { await pressPrimaryShortcut("a"); });
    });`);
    const support = join(workspace, "support");
    mkdirSync(support);
    writeFileSync(
      join(support, "keyboard.ts"),
      `export async function pressPrimaryShortcut(key) {
        await browser.action("key").down(primaryModifier).down(key).up(key).up(primaryModifier).perform();
      }`,
    );

    try {
      assert.deepEqual(validateScenarioLayout(root, ["platform-shortcuts"]), []);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it("rejects a second raw action inside the approved helper", () => {
    const { root, workspace } = scenarioRoot(`describe("@platform-shortcuts", () => {
      it("runs", async () => { await pressPrimaryShortcut("a"); });
    });`);
    const support = join(workspace, "support");
    mkdirSync(support);
    writeFileSync(
      join(support, "keyboard.ts"),
      `export async function pressPrimaryShortcut(key) {
        await browser.action("key").down(primaryModifier).down(key).up(key).up(primaryModifier).perform();
        await browser.action("key").down(Key.Shift).down(Key.F10).perform();
      }`,
    );

    try {
      assert.deepEqual(validateScenarioLayout(root, ["platform-shortcuts"]), [
        "support/keyboard.ts:2: raw keyboard action() is forbidden in real-input scenarios",
        "support/keyboard.ts:3: raw keyboard action() is forbidden in real-input scenarios",
      ]);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it("rejects raw browser.keys chords and variables outside the helper", () => {
    const { root, workspace } = scenarioRoot(`describe("@platform-shortcuts", () => {
      it("runs", async () => {
        await browser.keys([Key.Shift, Key.F10]);
        const chord = [primaryModifier, Key.ArrowRight];
        await browser.keys(chord);
      });
    });`);

    try {
      assert.deepEqual(validateScenarioLayout(root, ["platform-shortcuts"]), [
        "platform-shortcuts/platform-shortcuts.e2e.spec.ts:3: raw browser.keys chord is forbidden in real-input scenarios",
        "platform-shortcuts/platform-shortcuts.e2e.spec.ts:5: raw browser.keys chord is forbidden in real-input scenarios",
      ]);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it("does not confuse ordinary JavaScript keys methods with WebDriver input", () => {
    const { root, workspace } = scenarioRoot(`describe("@platform-shortcuts", () => {
      it("runs", async () => {
        const names = Object.keys(record);
        expect(names).toHaveLength(1);
      });
    });`);

    try {
      assert.deepEqual(validateScenarioLayout(root, ["platform-shortcuts"]), []);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it("rejects browser aliases and non-literal action kinds", () => {
    const { root, workspace } = scenarioRoot(`describe("@platform-shortcuts", () => {
      it("runs", async () => {
        const driver = browser;
        driver.action("key").down(Key.F10).perform();
        const { action } = browser;
        action("key").down(Key.F10).perform();
        const nativeDriver = globalThis.browser;
        nativeDriver.keys([Key.Shift, Key.F10]);
        const kind = "key";
        browser.action(kind).down(Key.F10).perform();
      });
    });`);

    try {
      assert.deepEqual(validateScenarioLayout(root, ["platform-shortcuts"]), [
        "platform-shortcuts/platform-shortcuts.e2e.spec.ts:3: browser alias is forbidden in real-input scenarios",
        "platform-shortcuts/platform-shortcuts.e2e.spec.ts:5: browser alias is forbidden in real-input scenarios",
        "platform-shortcuts/platform-shortcuts.e2e.spec.ts:7: browser alias is forbidden in real-input scenarios",
        "platform-shortcuts/platform-shortcuts.e2e.spec.ts:10: dynamic browser.action() is forbidden in real-input scenarios",
      ]);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it("rejects raw actions elsewhere in the approved helper module", () => {
    const { root, workspace } = scenarioRoot(`describe("@platform-shortcuts", () => {
      it("runs", async () => { await pressPrimaryShortcut("a"); });
    });`);
    const support = join(workspace, "support");
    mkdirSync(support);
    writeFileSync(
      join(support, "keyboard.ts"),
      `export async function pressPrimaryShortcut(key) {
        await browser.action("key").down(primaryModifier).down(key).up(key).up(primaryModifier).perform();
      }
      export async function unsafe(key) {
        await browser.action("key").down(Key.Shift).down(key).perform();
      }`,
    );

    try {
      assert.deepEqual(validateScenarioLayout(root, ["platform-shortcuts"]), [
        "support/keyboard.ts:5: raw keyboard action() is forbidden in real-input scenarios",
      ]);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it("does not exempt browser aliases inside the approved helper", () => {
    const { root, workspace } = scenarioRoot(`describe("@platform-shortcuts", () => {
      it("runs", async () => { await pressPrimaryShortcut("a"); });
    });`);
    const support = join(workspace, "support");
    mkdirSync(support);
    writeFileSync(
      join(support, "keyboard.ts"),
      `export async function pressPrimaryShortcut(key) {
        const driver = browser;
        await driver.action("key").down(primaryModifier).down(key).perform();
      }`,
    );

    try {
      assert.deepEqual(validateScenarioLayout(root, ["platform-shortcuts"]), [
        "support/keyboard.ts:2: browser alias is forbidden in real-input scenarios",
      ]);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });

  it("scans shared runtime modules outside scenarios and support", () => {
    const { root, workspace } = scenarioRoot(`import "../../src/unsafe";
      describe("@platform-shortcuts", () => {
        it("runs", async () => { await pressPrimaryShortcut("a"); });
      });`);
    const source = join(workspace, "src");
    mkdirSync(source);
    writeFileSync(
      join(source, "unsafe.ts"),
      `export async function unsafe() {
        await browser.action("key").down(Key.Shift).down(Key.F10).perform();
      }`,
    );

    try {
      assert.deepEqual(validateScenarioLayout(root, ["platform-shortcuts"]), [
        "src/unsafe.ts:2: raw keyboard action() is forbidden in real-input scenarios",
      ]);
    } finally {
      rmSync(workspace, { recursive: true, force: true });
    }
  });
});
