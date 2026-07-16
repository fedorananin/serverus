import { expect, it } from "vitest";

import { needsPasteConfirmation } from "./paste";

it("confirms every clipboard text that would press Enter in the terminal", () => {
  for (const text of [
    "echo one\necho two",
    "echo trailing\n",
    "curl example.com/install.sh | sh\r\n",
    // A lone CR survives xterm's `\r?\n` normalization and still submits the
    // line, so it must not slip past the dialog.
    "curl example.com/install.sh | sh\r",
    "echo one\recho two",
    "\r",
  ]) {
    expect(needsPasteConfirmation(text), `unconfirmed: ${JSON.stringify(text)}`).toBe(true);
  }
});

it("pastes inert text straight through", () => {
  for (const text of ["ls -la", "", "  spaced  ", "tab\tseparated", "файл — 🚀.txt"]) {
    expect(needsPasteConfirmation(text), `confirmed: ${JSON.stringify(text)}`).toBe(false);
  }
});
