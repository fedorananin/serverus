import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
  clipboardCommand,
  readSystemClipboard,
  type ClipboardExecutor,
} from "../support/system-clipboard";

describe("system clipboard reader", () => {
  it("uses a non-interactive command on every supported platform", () => {
    assert.deepEqual(clipboardCommand("darwin"), { command: "pbpaste", args: [] });
    assert.deepEqual(clipboardCommand("win32"), {
      command: "powershell.exe",
      args: ["-NoProfile", "-NonInteractive", "-Command", "Get-Clipboard -Raw"],
    });
    assert.deepEqual(clipboardCommand("linux"), {
      command: "xclip",
      args: ["-selection", "clipboard", "-out"],
    });
  });

  it("bounds the subprocess and strips only trailing newlines", () => {
    let timeout = 0;
    const execute: ClipboardExecutor = (_command, _args, options) => {
      timeout = options.timeout;
      return "https://example.test/object\r\n";
    };

    assert.equal(readSystemClipboard(execute, "darwin"), "https://example.test/object");
    assert.equal(timeout, 10_000);
  });

  it("does not copy clipboard contents from a subprocess error", () => {
    const execute: ClipboardExecutor = () => {
      throw new Error("secret clipboard payload");
    };
    assert.throws(
      () => readSystemClipboard(execute, "linux"),
      (error: unknown) =>
        error instanceof Error &&
        error.message === "The system clipboard could not be read on linux.",
    );
  });
});
