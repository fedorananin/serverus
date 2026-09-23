import { describe, expect, it } from "vitest";
import type { TransferSnapshot } from "$lib/api";
import {
  deletingPaths,
  enclosingDeletion,
  followUp,
  isWithin,
  TreeOpTracker,
} from "./tree-ops";

function op(
  id: string,
  remote_path: string,
  state: TransferSnapshot["state"],
  kind: TransferSnapshot["kind"] = "delete",
): TransferSnapshot {
  return {
    id,
    session_id: "s",
    kind,
    state,
    error: null,
    name: remote_path.split("/").pop() ?? "",
    local_path: "",
    remote_path,
    accelerated: false,
    scanning: false,
    done: 0,
    total: 0,
    speed_bps: 0,
  };
}

describe("isWithin", () => {
  it("matches the root itself and descendants only", () => {
    expect(isWithin("/srv/site", "/srv/site")).toBe(true);
    expect(isWithin("/srv/site/assets", "/srv/site")).toBe(true);
    expect(isWithin("/srv/site/assets", "/srv/site/")).toBe(true);
    expect(isWithin("/srv/site-old", "/srv/site")).toBe(false);
    expect(isWithin("/srv", "/srv/site")).toBe(false);
    expect(isWithin("/anything", "/")).toBe(true);
  });
});

describe("deleting paths", () => {
  it("lists unfinished deletes only", () => {
    const items = [
      op("1", "/srv/a", "running"),
      op("2", "/srv/b", "queued"),
      op("3", "/srv/c", "done"),
      op("4", "/srv/d", "running", "chmod"),
      op("5", "/srv/e", "paused"),
    ];
    expect([...deletingPaths(items)]).toEqual(["/srv/a", "/srv/b", "/srv/e"]);
  });

  it("finds the deletion the open folder is inside", () => {
    const deleting = new Set(["/srv/a", "/srv/b"]);
    expect(enclosingDeletion("/srv/b/deep/er", deleting)).toBe("/srv/b");
    expect(enclosingDeletion("/srv", deleting)).toBeNull();
  });
});

describe("follow-up after a finished operation", () => {
  it("refreshes the listing that contained the entry", () => {
    expect(followUp("/srv", op("1", "/srv/a", "done"))).toEqual({ kind: "refresh" });
    expect(followUp("/srv", op("1", "/srv/a", "error"))).toEqual({ kind: "refresh" });
    expect(followUp("/srv", op("1", "/srv/a", "done", "chmod"))).toEqual({ kind: "refresh" });
  });

  it("steps out of a folder that no longer exists", () => {
    expect(followUp("/srv/a/deep", op("1", "/srv/a", "done"))).toEqual({
      kind: "navigate",
      path: "/srv",
    });
  });

  it("stays inside a folder whose delete failed or was cancelled", () => {
    expect(followUp("/srv/a", op("1", "/srv/a", "cancelled"))).toEqual({ kind: "refresh" });
  });

  it("ignores operations elsewhere", () => {
    expect(followUp("/home", op("1", "/srv/a", "done"))).toBeNull();
    expect(followUp("/srv", op("1", "/srv/a/b", "done"))).toBeNull();
  });
});

describe("TreeOpTracker", () => {
  it("reports each operation once, when it finishes", () => {
    const tracker = new TreeOpTracker();
    expect(tracker.finished([op("1", "/srv/a", "running")])).toEqual([]);
    expect(tracker.finished([op("1", "/srv/a", "running")])).toEqual([]);
    const finished = tracker.finished([op("1", "/srv/a", "done")]);
    expect(finished.map((item) => item.id)).toEqual(["1"]);
    expect(tracker.finished([op("1", "/srv/a", "done")])).toEqual([]);
  });

  it("reports an operation that finished between two snapshots", () => {
    const tracker = new TreeOpTracker();
    expect(tracker.finished([op("7", "/srv/x", "done")]).map((item) => item.id)).toEqual(["7"]);
  });

  it("ignores byte transfers and forgets cleared items", () => {
    const tracker = new TreeOpTracker();
    expect(tracker.finished([op("u", "/srv/f", "done", "upload")])).toEqual([]);
    tracker.finished([op("1", "/srv/a", "done")]);
    tracker.finished([]);
    // A retried item that shows up again finished is reported again.
    expect(tracker.finished([op("1", "/srv/a", "done")]).map((item) => item.id)).toEqual(["1"]);
  });
});
