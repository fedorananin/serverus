// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { expect, it, vi } from "vitest";
import TerminalPanel from "./TerminalPanel.svelte";

vi.mock("./TerminalView.svelte", async () => import("../../test/Stub.svelte"));

it("exposes terminal channels as tabs with real close buttons", async () => {
  render(TerminalPanel, { sessionId: "session-a" });

  expect(screen.getByRole("tab", { name: "Terminal 1" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await fireEvent.click(screen.getByRole("button", { name: "New terminal" }));

  const second = screen.getByRole("tab", { name: "Terminal 2" });
  expect(second).toHaveAttribute("aria-selected", "true");
  await fireEvent.click(screen.getByRole("button", { name: "Close terminal 2" }));

  expect(screen.queryByRole("tab", { name: "Terminal 2" })).not.toBeInTheDocument();
  expect(screen.getByRole("tab", { name: "Terminal 1" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
});
