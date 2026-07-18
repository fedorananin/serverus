// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { describe, expect, it, vi } from "vitest";

import ConfirmDialog from "./ConfirmDialog.svelte";

describe("ConfirmDialog", () => {
  it("focuses the confirm button so pane hotkeys stop firing", () => {
    render(ConfirmDialog, {
      title: "Delete",
      message: "Delete alpha.txt?",
      onconfirm: vi.fn(),
      onclose: vi.fn(),
    });

    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Delete" }));
  });

  it("confirms on Enter even when focus is elsewhere", async () => {
    const onconfirm = vi.fn();
    const onclose = vi.fn();
    render(ConfirmDialog, {
      title: "Delete",
      message: "Delete alpha.txt?",
      onconfirm,
      onclose,
    });

    (document.activeElement as HTMLElement | null)?.blur();
    await fireEvent.keyDown(document.body, { key: "Enter" });

    expect(onconfirm).toHaveBeenCalledOnce();
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("leaves Enter to a focused Cancel button instead of confirming", async () => {
    const onconfirm = vi.fn();
    render(ConfirmDialog, {
      title: "Delete",
      message: "Delete alpha.txt?",
      onconfirm,
      onclose: vi.fn(),
    });

    const cancel = screen.getByRole("button", { name: "Cancel" });
    cancel.focus();
    await fireEvent.keyDown(cancel, { key: "Enter" });

    expect(onconfirm).not.toHaveBeenCalled();
  });
});
