// @vitest-environment jsdom

import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { RemoteEntry } from "$lib/api";
import { PaneController } from "$lib/stores/pane.svelte";
import type { RemoteTreeActions } from "$lib/stores/remote-tree-ops.svelte";
import FilePane from "./FilePane.svelte";

function entry(name: string, is_dir: boolean, is_symlink = false): RemoteEntry {
  return {
    name,
    path: `/srv/${name}`,
    is_dir,
    is_symlink,
    size: 0,
    mtime: null,
    permissions: 0o755,
  };
}

function remotePane(entries: RemoteEntry[]) {
  const pane = new PaneController("remote", "session-a", false);
  pane.path = "/srv";
  pane.entries = entries;
  return pane;
}

function actions(): RemoteTreeActions {
  return { delete: vi.fn(async () => {}), chmod: vi.fn(async () => {}) };
}

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", class {
    observe() {}
    unobserve() {}
    disconnect() {}
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("remote tree operations", () => {
  it("hands a confirmed delete to the transfer panel", async () => {
    const site = entry("site", true);
    const treeActions = actions();
    render(FilePane, { pane: remotePane([site]), title: "Remote", ontransfer: vi.fn(), treeActions });

    await fireEvent.click(screen.getByRole("option", { name: "site" }));
    await fireEvent.keyDown(screen.getByRole("listbox", { name: "Remote files" }), {
      key: "Delete",
    });
    const dialog = screen.getByRole("dialog");
    await fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));

    expect(treeActions.delete).toHaveBeenCalledWith([site]);
  });

  it("dims rows being deleted and flags a folder that is being deleted", () => {
    const pane = remotePane([entry("site", true), entry("keep.txt", false)]);
    pane.deleting = new Set(["/srv/site"]);
    const view = render(FilePane, { pane, title: "Remote", ontransfer: vi.fn() });

    expect(screen.getByRole("option", { name: "site" })).toHaveClass("deleting");
    expect(screen.getByRole("option", { name: "keep.txt" })).not.toHaveClass("deleting");
    expect(screen.queryByText("This folder is being deleted…")).not.toBeInTheDocument();
    view.unmount();

    const inside = remotePane([]);
    inside.path = "/srv/site/assets";
    inside.deleting = new Set(["/srv/site"]);
    render(FilePane, { pane: inside, title: "Remote", ontransfer: vi.fn() });
    expect(screen.getByRole("status")).toHaveTextContent("This folder is being deleted…");
  });
});
