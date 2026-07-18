import { describe, expect, it } from "vitest";

import type { RemoteEntry } from "$lib/api";
import { compareDirectoryEntries } from "$lib/directory-comparison";

function entry(
  name: string,
  overrides: Partial<RemoteEntry> = {},
): RemoteEntry {
  return {
    name,
    path: `/fixture/${name}`,
    is_dir: false,
    is_symlink: false,
    size: 10,
    mtime: 100,
    permissions: 0o644,
    ...overrides,
  };
}

describe("compareDirectoryEntries", () => {
  it("classifies matching, changed, and one-sided entries for both panes", () => {
    const comparison = compareDirectoryEntries(
      [entry("same.txt"), entry("changed.txt"), entry("local.txt")],
      [entry("same.txt"), entry("changed.txt", { size: 11 }), entry("remote.txt")],
    );

    expect(Object.fromEntries(comparison.localStatuses)).toEqual({
      "same.txt": "matching",
      "changed.txt": "different",
      "local.txt": "local-only",
    });
    expect(Object.fromEntries(comparison.remoteStatuses)).toEqual({
      "same.txt": "matching",
      "changed.txt": "different",
      "remote.txt": "remote-only",
    });
    expect(comparison.summary).toEqual({
      matching: 1,
      different: 1,
      localOnly: 1,
      remoteOnly: 1,
    });
  });

  it("compares file type, symlink state, size, and known modification time", () => {
    const local = [
      entry("type"),
      entry("symlink"),
      entry("size"),
      entry("mtime"),
      entry("unknown-mtime", { mtime: null }),
    ];
    const remote = [
      entry("type", { is_dir: true }),
      entry("symlink", { is_symlink: true }),
      entry("size", { size: 20 }),
      entry("mtime", { mtime: 101 }),
      entry("unknown-mtime", { mtime: 500 }),
    ];

    const comparison = compareDirectoryEntries(local, remote);

    expect([...comparison.localStatuses.values()]).toEqual([
      "different",
      "different",
      "different",
      "different",
      "matching",
    ]);
  });

  it("ignores mtime when asked — S3's server-managed LastModified is not comparable", () => {
    const local = [entry("uploaded.txt", { mtime: 100 }), entry("shrunk.txt", { size: 10 })];
    const remote = [
      entry("uploaded.txt", { mtime: 999_999 }),
      entry("shrunk.txt", { size: 11, mtime: 999_999 }),
    ];

    const comparison = compareDirectoryEntries(local, remote, { ignoreMtime: true });

    expect(comparison.localStatuses.get("uploaded.txt")).toBe("matching");
    expect(comparison.localStatuses.get("shrunk.txt")).toBe("different");
  });

  it("compares coarse remote mtimes at minute precision — FTP LIST has no seconds", () => {
    const noon = 12 * 3600; // 12:00:00
    const local = [
      entry("same-minute.txt", { mtime: noon + 37 }), // 12:00:37 locally
      entry("next-minute.txt", { mtime: noon + 61 }), // 12:01:01 locally
    ];
    const remote = [
      entry("same-minute.txt", { mtime: noon }), // listed as 12:00
      entry("next-minute.txt", { mtime: noon }), // listed as 12:00
    ];

    const comparison = compareDirectoryEntries(local, remote, { coarseRemoteMtime: true });

    expect(comparison.localStatuses.get("same-minute.txt")).toBe("matching");
    expect(comparison.localStatuses.get("next-minute.txt")).toBe("different");
  });

  it("compares date-only coarse remote mtimes at day precision", () => {
    const day = 24 * 3600;
    const local = [
      entry("same-day.txt", { mtime: 5 * day + 15 * 3600 }), // afternoon of day 5
      entry("other-day.txt", { mtime: 6 * day + 15 * 3600 }),
      // Genuinely-midnight local mtime must not loosen a minute-precise stamp.
      entry("midnight-local.txt", { mtime: 5 * day }),
    ];
    const remote = [
      entry("same-day.txt", { mtime: 5 * day }), // listed as a bare date
      entry("other-day.txt", { mtime: 5 * day }),
      entry("midnight-local.txt", { mtime: 5 * day + 15 * 3600 + 60 }),
    ];

    const comparison = compareDirectoryEntries(local, remote, { coarseRemoteMtime: true });

    expect(comparison.localStatuses.get("same-day.txt")).toBe("matching");
    expect(comparison.localStatuses.get("other-day.txt")).toBe("different");
    expect(comparison.localStatuses.get("midnight-local.txt")).toBe("different");
  });

  it("keeps exact mtime comparison when the remote listing is second-precise", () => {
    const comparison = compareDirectoryEntries(
      [entry("synced.txt", { mtime: 100 })],
      [entry("synced.txt", { mtime: 130 })],
    );

    expect(comparison.localStatuses.get("synced.txt")).toBe("different");
  });

  it("does not claim to compare directory contents from directory metadata", () => {
    const comparison = compareDirectoryEntries(
      [entry("assets", { is_dir: true, size: 0, mtime: 100 })],
      [entry("assets", { is_dir: true, size: 500, mtime: 900 })],
    );

    expect(comparison.localStatuses.get("assets")).toBe("matching");
    expect(comparison.summary.matching).toBe(1);
  });

  it("keeps names case-sensitive", () => {
    const comparison = compareDirectoryEntries([entry("README.md")], [entry("readme.md")]);

    expect(comparison.summary).toEqual({
      matching: 0,
      different: 0,
      localOnly: 1,
      remoteOnly: 1,
    });
  });

  it("keeps name access linear for large open folders", () => {
    let nameReads = 0;
    const countedEntry = (name: string): RemoteEntry => {
      const value = entry(name);
      return Object.defineProperty(value, "name", {
        configurable: true,
        enumerable: true,
        get() {
          nameReads += 1;
          return name;
        },
      });
    };
    const count = 2_000;
    const local = Array.from({ length: count }, (_, index) => countedEntry(`file-${index}`));
    const remote = Array.from({ length: count }, (_, index) => countedEntry(`file-${index}`));

    const comparison = compareDirectoryEntries(local, remote);

    expect(comparison.summary.matching).toBe(count);
    expect(nameReads).toBeLessThanOrEqual(count * 4);
  });
});
