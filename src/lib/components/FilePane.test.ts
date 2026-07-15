// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { RemoteEntry } from "$lib/api";
import { PaneController } from "$lib/stores/pane.svelte";
import FilePane from "./FilePane.svelte";

const originalClipboard = Object.getOwnPropertyDescriptor(navigator, "clipboard");
const object: RemoteEntry = {
  name: "report.txt",
  path: "bucket/report.txt",
  is_dir: false,
  is_symlink: false,
  size: 12,
  mtime: null,
  permissions: null,
};

function pane() {
  const controller = new PaneController("remote", "session-a", false, true);
  controller.path = "bucket";
  controller.entries = [object];
  return controller;
}

afterEach(() => {
  if (originalClipboard) Object.defineProperty(navigator, "clipboard", originalClipboard);
  else Reflect.deleteProperty(navigator, "clipboard");
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("S3 public URL clipboard feedback", () => {
  beforeEach(() => {
    vi.stubGlobal("ResizeObserver", class {
      observe() {}
      unobserve() {}
      disconnect() {}
    });
  });

  it("reports success only after the system clipboard write completes", async () => {
    let finishWrite: (() => void) | undefined;
    const writeText = vi.fn(() => new Promise<void>((resolve) => (finishWrite = resolve)));
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    render(FilePane, {
      pane: pane(),
      title: "Remote",
      ontransfer: vi.fn(),
      publicUrl: () => "https://cdn.example/report.txt",
    });

    await fireEvent.click(screen.getByRole("option", { name: object.name }));
    await fireEvent.click(screen.getByRole("button", { name: "Remote pane actions" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Copy public URL" }));

    expect(writeText).toHaveBeenCalledWith("https://cdn.example/report.txt");
    expect(screen.queryByText("Public URL copied")).not.toBeInTheDocument();
    finishWrite?.();
    expect(await screen.findByRole("status")).toHaveTextContent("Public URL copied");
  });

  it("shows a visible error when the system clipboard rejects the write", async () => {
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: vi.fn().mockRejectedValue(new Error("Clipboard denied")) },
    });
    render(FilePane, {
      pane: pane(),
      title: "Remote",
      ontransfer: vi.fn(),
      publicUrl: () => "https://cdn.example/report.txt",
    });

    await fireEvent.click(screen.getByRole("option", { name: object.name }));
    await fireEvent.click(screen.getByRole("button", { name: "Remote pane actions" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Copy public URL" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Copy public URL failed: Clipboard denied",
    );
  });
});

describe("file actions keyboard access", () => {
  beforeEach(() => {
    vi.stubGlobal("ResizeObserver", class {
      observe() {}
      unobserve() {}
      disconnect() {}
    });
  });

  it("opens the selected file actions when the renderer receives Shift+F10", async () => {
    render(FilePane, {
      pane: pane(),
      title: "Remote",
      ontransfer: vi.fn(),
    });

    await fireEvent.click(screen.getByRole("option", { name: object.name }));
    await fireEvent.keyDown(screen.getByRole("listbox", { name: "Remote files" }), {
      key: "F10",
      shiftKey: true,
    });

    expect(screen.getByRole("menuitem", { name: "← Download" })).toBeInTheDocument();
  });
});
