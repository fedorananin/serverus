// @vitest-environment jsdom

import { fireEvent, render, screen, within } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { RemoteEntry } from "$lib/api";
import { PaneController } from "$lib/stores/pane.svelte";
import FilePane from "./FilePane.svelte";

const alpha: RemoteEntry = {
  name: "alpha.txt",
  path: "/root/alpha.txt",
  is_dir: false,
  is_symlink: false,
  size: 20,
  mtime: 20,
  permissions: 0o644,
};
const beta: RemoteEntry = {
  name: "beta.txt",
  path: "/root/beta.txt",
  is_dir: false,
  is_symlink: false,
  size: 10,
  mtime: 10,
  permissions: 0o600,
};

function remotePane() {
  const pane = new PaneController("remote", "session-a", false);
  pane.path = "/root";
  pane.entries = [alpha, beta];
  return pane;
}

beforeEach(() => {
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
});

describe("FilePane interactions", () => {
  it("keeps sorting, filtering, selection and status feedback connected", async () => {
    render(FilePane, {
      pane: remotePane(),
      title: "Remote",
      ontransfer: vi.fn(),
    });

    await fireEvent.click(screen.getByRole("button", { name: /^Size/u }));
    expect(screen.getAllByRole("option").map((row) => row.getAttribute("aria-label"))).toEqual([
      beta.name,
      alpha.name,
    ]);

    await fireEvent.input(screen.getByPlaceholderText("Filter"), {
      target: { value: "alpha" },
    });
    expect(screen.getAllByRole("option")).toHaveLength(1);
    await fireEvent.click(screen.getByRole("option", { name: alpha.name }));
    expect(screen.getByRole("option", { name: alpha.name })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByText("1 items, 1 selected")).toBeInTheDocument();
  });

  it("labels comparison results without relying on color alone", () => {
    const pane = remotePane();
    pane.comparisonStatuses = new Map([
      [alpha.name, "remote-only"],
      [beta.name, "matching"],
    ]);
    render(FilePane, { pane, title: "Remote", ontransfer: vi.fn() });

    const remoteOnly = screen.getByRole("option", { name: alpha.name });
    expect(remoteOnly).toHaveAttribute("data-comparison-status", "remote-only");
    expect(remoteOnly).toHaveAccessibleDescription("Remote Only");
    expect(screen.getByTitle("Remote Only")).toHaveTextContent("R");
    expect(screen.getByRole("option", { name: beta.name })).toHaveAccessibleDescription(
      "Same Metadata",
    );
  });

  it("keeps path editing and toolbar actions connected to the pane", async () => {
    const pane = remotePane();
    const navigate = vi.spyOn(pane, "navigate").mockResolvedValue();
    const up = vi.spyOn(pane, "up").mockResolvedValue();
    const refresh = vi.spyOn(pane, "refresh").mockResolvedValue();
    const { container } = render(FilePane, {
      pane,
      title: "Remote",
      ontransfer: vi.fn(),
    });

    await fireEvent.click(screen.getByRole("button", { name: "Remote path" }));
    const input = screen.getByLabelText("Remote path input");
    await fireEvent.input(input, { target: { value: "/next" } });
    await fireEvent.submit(input.closest("form")!);

    expect(navigate).toHaveBeenCalledWith("/next");
    await fireEvent.click(screen.getByRole("button", { name: "Up" }));
    await fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
    expect(up).toHaveBeenCalledOnce();
    expect(refresh).toHaveBeenCalledOnce();
    expect(container.querySelector("[data-pane='remote']")).toBeInTheDocument();
  });

  it("keeps transfer and rename actions attached to the current selection", async () => {
    const pane = remotePane();
    const rename = vi.spyOn(pane, "rename").mockResolvedValue();
    const ontransfer = vi.fn();
    render(FilePane, { pane, title: "Remote", ontransfer });

    await fireEvent.click(screen.getByRole("option", { name: alpha.name }));
    await fireEvent.click(screen.getByRole("button", { name: "Remote pane actions" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "← Download" }));
    expect(ontransfer).toHaveBeenCalledWith([alpha]);

    await fireEvent.click(screen.getByRole("button", { name: "Remote pane actions" }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Rename…" }));
    const dialog = screen.getByRole("dialog", { name: "Rename" });
    const input = within(dialog).getByRole("textbox");
    await fireEvent.input(input, { target: { value: "renamed.txt" } });
    await fireEvent.click(within(dialog).getByRole("button", { name: "Rename" }));

    expect(rename).toHaveBeenCalledWith(alpha, "renamed.txt");
  });
});
