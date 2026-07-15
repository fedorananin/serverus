import assert from "node:assert/strict";
import { describe, it } from "node:test";
import { Key } from "webdriverio";

import {
  PRIMARY_SHORTCUT_KEYS,
  pressPrimaryShortcut,
} from "../support/keyboard";

describe("primary shortcut helper", () => {
  it("exposes only the regular keys exercised by the automated scenarios", () => {
    assert.deepEqual(PRIMARY_SHORTCUT_KEYS, ["a", "t", "w", "1", "2", ","]);
  });

  it("rejects a WebDriver special key before it reaches the browser", async () => {
    await assert.rejects(
      pressPrimaryShortcut(Key.F10 as never),
      new Error("The scenario requested an unsupported primary shortcut key."),
    );
  });
});
