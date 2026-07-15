// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { expect, it, vi } from "vitest";
import TerminalPanel from "./TerminalPanel.svelte";

vi.mock("./TerminalView.svelte", async () => import("../../test/Stub.svelte"));

it("exposes terminal channels and their close actions as real buttons", async () => {
  render(TerminalPanel, { sessionId: "session-a" });

  expect(screen.getByRole("button", { name: "Terminal 1" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await fireEvent.click(screen.getByRole("button", { name: "New terminal" }));

  const second = screen.getByRole("button", { name: "Terminal 2" });
  expect(second).toHaveAttribute("aria-pressed", "true");
  await fireEvent.click(screen.getByRole("button", { name: "Close terminal 2" }));

  expect(screen.queryByRole("button", { name: "Terminal 2" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Terminal 1" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
});
