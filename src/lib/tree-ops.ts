// How queued recursive operations (delete / chmod) relate to what a remote
// file pane shows: which rows are being deleted, whether the open folder
// is inside a deletion, and when a finished operation must refresh the pane.

import type { TransferSnapshot, TransferState } from "$lib/api";
import { parentPath } from "$lib/format";

const LIVE: ReadonlySet<TransferState> = new Set(["queued", "running", "paused", "conflict"]);

export function isTreeOp(item: TransferSnapshot): boolean {
  return item.kind === "delete" || item.kind === "chmod";
}

export function isLive(state: TransferState): boolean {
  return LIVE.has(state);
}

/** `path` is `root` itself or lies below it (POSIX remote paths). */
export function isWithin(path: string, root: string): boolean {
  const base = root.length > 1 ? root.replace(/\/+$/, "") : root;
  return path === base || path.startsWith(base === "/" ? "/" : `${base}/`);
}

/** Remote paths with a delete that has not finished yet. */
export function deletingPaths(items: TransferSnapshot[]): Set<string> {
  return new Set(
    items
      .filter((item) => item.kind === "delete" && isLive(item.state))
      .map((item) => item.remote_path),
  );
}

/** The deletion root the open folder lies in, if any. */
export function enclosingDeletion(path: string, deleting: Iterable<string>): string | null {
  for (const root of deleting) {
    if (isWithin(path, root)) return root;
  }
  return null;
}

export type PaneFollowUp = { kind: "refresh" } | { kind: "navigate"; path: string } | null;

/** What a pane showing `panePath` must do once `item` has finished. */
export function followUp(panePath: string, item: TransferSnapshot): PaneFollowUp {
  if (item.kind === "delete" && item.state === "done" && isWithin(panePath, item.remote_path)) {
    // The open folder is gone: step out to where the deleted entry lived.
    return { kind: "navigate", path: parentPath(item.remote_path) };
  }
  if (parentPath(item.remote_path) === panePath || isWithin(panePath, item.remote_path)) {
    return { kind: "refresh" };
  }
  return null;
}

/**
 * Tracks tree operations between progress snapshots and reports the ones
 * that finished since the last call. An item first seen already finished
 * counts too: a single-file delete often completes between two snapshots.
 */
export class TreeOpTracker {
  private readonly seen = new Map<string, TransferState>();

  finished(items: TransferSnapshot[]): TransferSnapshot[] {
    const done: TransferSnapshot[] = [];
    const present = new Set<string>();
    for (const item of items) {
      if (!isTreeOp(item)) continue;
      present.add(item.id);
      const before = this.seen.get(item.id);
      this.seen.set(item.id, item.state);
      if (!isLive(item.state) && (before === undefined || isLive(before))) done.push(item);
    }
    for (const id of [...this.seen.keys()]) {
      if (!present.has(id)) this.seen.delete(id);
    }
    return done;
  }
}
