// Deep folder comparison for one session's file view: runs the backend
// subtree walk for every directory pair the flat comparison found, a few at
// a time, and publishes per-directory statuses as they resolve. Restarting
// (new listings, toggling) cancels the previous batch both here (generation
// guard) and on the backend (epoch bump), so stale walks stop burning the
// connection and stale results are discarded.

import { commands, unwrap } from "$lib/api";
import type { RemoteEntry } from "$lib/api";
import type { DeepDirectoryStatus } from "$lib/directory-comparison";

/** A same-named directory present in both panes. */
export interface DirectoryPair {
  name: string;
  localPath: string;
  remotePath: string;
}

/** The directory pairs of two open listings — same name, a real (non-symlink)
 *  directory on both sides. Only these get a deep walk. */
export function directoryPairs(
  localEntries: readonly RemoteEntry[],
  remoteEntries: readonly RemoteEntry[],
): DirectoryPair[] {
  const remoteDirs = new Map(
    remoteEntries.filter((e) => e.is_dir && !e.is_symlink).map((e) => [e.name, e]),
  );
  return localEntries
    .filter((e) => e.is_dir && !e.is_symlink && remoteDirs.has(e.name))
    .map((e) => ({ name: e.name, localPath: e.path, remotePath: remoteDirs.get(e.name)!.path }));
}

export interface SubtreeComparisonRequest {
  pairs: DirectoryPair[];
  ignoreMtime: boolean;
  coarseRemoteMtime: boolean;
  includeHidden: boolean;
  hideLocalJunk: boolean;
}

/** Directory listings are cheap but serialized per connection — a small
 *  fan-out overlaps round trips without starving transfers. */
const CONCURRENCY = 3;

export class SubtreeComparisonController {
  /** Deep status per directory name; missing = not a compared pair. */
  statuses = $state<ReadonlyMap<string, DeepDirectoryStatus>>(new Map());

  private generation = 0;

  constructor(private readonly sessionId: string) {}

  /** Compare the given pairs, replacing (and cancelling) any previous batch. */
  async run(request: SubtreeComparisonRequest): Promise<void> {
    const generation = await this.restart();
    if (generation !== this.generation) return;
    this.statuses = new Map(request.pairs.map((pair) => [pair.name, "comparing"]));
    const queue = [...request.pairs];
    const worker = async () => {
      for (;;) {
        const pair = queue.shift();
        if (!pair || generation !== this.generation) return;
        let status: DeepDirectoryStatus;
        try {
          status = await unwrap(
            commands.remoteCompareSubtree(
              this.sessionId,
              pair.localPath,
              pair.remotePath,
              request.ignoreMtime,
              request.coarseRemoteMtime,
              request.includeHidden,
              request.hideLocalJunk,
            ),
          );
        } catch {
          // Cancelled walks also land here; the generation guard below
          // drops their result either way.
          status = "unknown";
        }
        if (generation !== this.generation) return;
        const next = new Map(this.statuses);
        next.set(pair.name, status);
        this.statuses = next;
      }
    };
    const workers = Math.min(CONCURRENCY, queue.length);
    await Promise.all(Array.from({ length: workers }, worker));
  }

  /** Cancel any in-flight batch and clear all statuses. */
  async stop(): Promise<void> {
    const generation = await this.restart();
    if (generation !== this.generation) return;
    this.statuses = new Map();
  }

  private async restart(): Promise<number> {
    const generation = ++this.generation;
    try {
      await unwrap(commands.remoteCompareCancel(this.sessionId));
    } catch {
      // The session may already be gone; in-flight walks die with it.
    }
    return generation;
  }
}
