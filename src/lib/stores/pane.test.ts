import { describe, expect, it } from "vitest";

import type { RemoteEntry } from "$lib/api";
import { compareDirectoryEntries } from "$lib/directory-comparison";
import { PaneController } from "$lib/stores/pane.svelte";

function entry(name: string): RemoteEntry {
  return {
    name,
    path: `/fixture/${name}`,
    is_dir: false,
    is_symlink: false,
    size: 1,
    mtime: 100,
    permissions: 0o644,
  };
}

describe("PaneController directory comparison", () => {
  it("keeps hidden-file visibility independent from comparison state", () => {
    const local = new PaneController("local", null, false);
    const remote = new PaneController("remote", "session-a", true);
    local.entries = [entry("visible.txt"), entry(".hidden.txt")];
    remote.entries = [entry("visible.txt"), entry(".hidden.txt")];

    const comparison = compareDirectoryEntries(local.entries, remote.entries);
    local.comparisonStatuses = comparison.localStatuses;
    remote.comparisonStatuses = comparison.remoteStatuses;

    expect(comparison.summary).toMatchObject({ matching: 2, localOnly: 0, remoteOnly: 0 });
    expect(local.visible.map(({ name }) => name)).toEqual(["visible.txt"]);
    expect(remote.visible.map(({ name }) => name)).toEqual([".hidden.txt", "visible.txt"]);
    expect(remote.comparisonStatuses.get(".hidden.txt")).toBe("matching");
  });

  it("shows only non-matching entries when the comparison filter is enabled", () => {
    const pane = new PaneController("local", null, false);
    pane.entries = [entry("matching.txt"), entry("different.txt"), entry("local-only.txt")];
    pane.comparisonStatuses = new Map([
      ["matching.txt", "matching"],
      ["different.txt", "different"],
      ["local-only.txt", "local-only"],
    ]);
    pane.comparisonDifferencesOnly = true;

    expect(pane.visible.map(({ name }) => name)).toEqual(["different.txt", "local-only.txt"]);
  });

  it("does not hide entries after comparison mode is cleared", () => {
    const pane = new PaneController("remote", "session-a", false);
    pane.entries = [entry("matching.txt"), entry("remote-only.txt")];
    pane.comparisonStatuses = null;
    pane.comparisonDifferencesOnly = true;

    expect(pane.visible.map(({ name }) => name)).toEqual(["matching.txt", "remote-only.txt"]);
  });
});
