import type { RemoteEntry, SubtreeStatus } from "$lib/api";

export type DirectoryComparisonStatus =
  | "matching"
  | "different"
  | "local-only"
  | "remote-only";

/** OS metadata junk the "hide local junk" panel setting suppresses in the
 *  local pane and on the local side of comparison (case-insensitive —
 *  Windows writes both `Thumbs.db` and `thumbs.db` in the wild). Mirrored
 *  by `is_local_junk` in `serverus-domain::fs_compare` — keep in lockstep. */
export function isLocalJunk(name: string): boolean {
  const lower = name.toLowerCase();
  return lower === ".ds_store" || lower === "thumbs.db";
}

/** Status of one directory pair while (or after) its contents are compared
 *  by the backend walk: a `SubtreeStatus`, or "comparing" while in flight. */
export type DeepDirectoryStatus = SubtreeStatus | "comparing";

/** What a pane row can display: the flat metadata statuses plus the deep
 *  folder states ("unknown" = could not be verified, e.g. tree too large). */
export type EntryComparisonStatus = DirectoryComparisonStatus | DeepDirectoryStatus;

export interface DirectoryComparisonSummary {
  matching: number;
  different: number;
  localOnly: number;
  remoteOnly: number;
  /** Directory pairs whose contents are still being compared. */
  comparing: number;
  /** Directory pairs that could not be verified (too large, unreadable). */
  unknown: number;
}

export interface DirectoryComparison {
  localStatuses: Map<string, EntryComparisonStatus>;
  remoteStatuses: Map<string, EntryComparisonStatus>;
  summary: DirectoryComparisonSummary;
}

export interface DirectoryComparisonOptions {
  /** Skip modification-time comparison. Set for backends whose listed mtime
   *  is server-managed upload time (S3 LastModified) rather than a content
   *  stamp transfers can preserve — comparing it would mark every uploaded
   *  file "different" forever. */
  ignoreMtime?: boolean;
  /** Compare mtime at the precision the remote listing actually carries.
   *  Set for FTP: LIST timestamps have no seconds for recent files, and
   *  only a date for files older than ~6 months — comparing them exactly
   *  against a second-precise local mtime flags nearly every synced file
   *  "different". */
  coarseRemoteMtime?: boolean;
}

const MINUTE = 60;
const DAY = 24 * 60 * 60;

function mtimesMatch(local: number, remote: number, coarseRemote: boolean): boolean {
  if (!coarseRemote) return local === remote;
  // A midnight-exact remote stamp is a date-only LIST entry; anything else
  // came with minute precision.
  const precision = remote % DAY === 0 ? DAY : MINUTE;
  return Math.floor(local / precision) === Math.floor(remote / precision);
}

function entriesMatch(
  local: RemoteEntry,
  remote: RemoteEntry,
  options: DirectoryComparisonOptions,
): boolean {
  if (local.is_dir !== remote.is_dir || local.is_symlink !== remote.is_symlink) return false;
  if (local.is_dir) return true;
  if (local.size !== remote.size) return false;
  if (options.ignoreMtime) return true;
  if (local.mtime === null || remote.mtime === null) return true;
  return mtimesMatch(local.mtime, remote.mtime, options.coarseRemoteMtime ?? false);
}

/** Compare the entries already loaded for two open folders.
 *
 * One map lookup per entry keeps the operation O(local + remote) in time and
 * O(remote) in auxiliary space. Directory contents are deliberately not
 * inferred from unreliable directory size/mtime metadata.
 */
export function compareDirectoryEntries(
  localEntries: readonly RemoteEntry[],
  remoteEntries: readonly RemoteEntry[],
  options: DirectoryComparisonOptions = {},
): DirectoryComparison {
  const localStatuses = new Map<string, EntryComparisonStatus>();
  const remoteStatuses = new Map<string, EntryComparisonStatus>();
  const remoteByName = new Map(remoteEntries.map((entry) => [entry.name, entry]));
  const summary: DirectoryComparisonSummary = {
    matching: 0,
    different: 0,
    localOnly: 0,
    remoteOnly: 0,
    comparing: 0,
    unknown: 0,
  };

  for (const local of localEntries) {
    const name = local.name;
    const remote = remoteByName.get(name);
    if (!remote) {
      localStatuses.set(name, "local-only");
      summary.localOnly += 1;
      continue;
    }
    const status = entriesMatch(local, remote, options) ? "matching" : "different";
    localStatuses.set(name, status);
    remoteStatuses.set(name, status);
    summary[status] += 1;
  }

  for (const remote of remoteEntries) {
    const name = remote.name;
    if (remoteStatuses.has(name)) continue;
    remoteStatuses.set(name, "remote-only");
    summary.remoteOnly += 1;
  }

  return { localStatuses, remoteStatuses, summary };
}

/** Overlay deep folder-walk results onto a flat comparison.
 *
 * `deep` is keyed by directory name and only ever contains names that are a
 * directory pair in the flat comparison (where the flat status is always
 * "matching" — flat rules cannot see directory contents). A deep "matching"
 * therefore upgrades nothing; "comparing", "different" and "unknown" replace
 * the flat status on both sides.
 */
export function applyDeepStatuses(
  flat: DirectoryComparison,
  deep: ReadonlyMap<string, DeepDirectoryStatus>,
): DirectoryComparison {
  if (deep.size === 0) return flat;
  const localStatuses = new Map(flat.localStatuses);
  const remoteStatuses = new Map(flat.remoteStatuses);
  const summary = { ...flat.summary };
  for (const [name, status] of deep) {
    if (localStatuses.get(name) !== "matching") continue;
    if (status === "matching") continue;
    localStatuses.set(name, status);
    remoteStatuses.set(name, status);
    summary.matching -= 1;
    summary[status] += 1;
  }
  return { localStatuses, remoteStatuses, summary };
}
