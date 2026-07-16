import type { RemoteEntry } from "$lib/api";

export type DirectoryComparisonStatus =
  | "matching"
  | "different"
  | "local-only"
  | "remote-only";

export interface DirectoryComparisonSummary {
  matching: number;
  different: number;
  localOnly: number;
  remoteOnly: number;
}

export interface DirectoryComparison {
  localStatuses: Map<string, DirectoryComparisonStatus>;
  remoteStatuses: Map<string, DirectoryComparisonStatus>;
  summary: DirectoryComparisonSummary;
}

export interface DirectoryComparisonOptions {
  /** Skip modification-time comparison. Set for backends whose listed mtime
   *  is server-managed upload time (S3 LastModified) rather than a content
   *  stamp transfers can preserve — comparing it would mark every uploaded
   *  file "different" forever. */
  ignoreMtime?: boolean;
}

function entriesMatch(local: RemoteEntry, remote: RemoteEntry, ignoreMtime: boolean): boolean {
  if (local.is_dir !== remote.is_dir || local.is_symlink !== remote.is_symlink) return false;
  if (local.is_dir) return true;
  if (local.size !== remote.size) return false;
  if (ignoreMtime) return true;
  return local.mtime === null || remote.mtime === null || local.mtime === remote.mtime;
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
  const ignoreMtime = options.ignoreMtime ?? false;
  const localStatuses = new Map<string, DirectoryComparisonStatus>();
  const remoteStatuses = new Map<string, DirectoryComparisonStatus>();
  const remoteByName = new Map(remoteEntries.map((entry) => [entry.name, entry]));
  const summary: DirectoryComparisonSummary = {
    matching: 0,
    different: 0,
    localOnly: 0,
    remoteOnly: 0,
  };

  for (const local of localEntries) {
    const name = local.name;
    const remote = remoteByName.get(name);
    if (!remote) {
      localStatuses.set(name, "local-only");
      summary.localOnly += 1;
      continue;
    }
    const status = entriesMatch(local, remote, ignoreMtime) ? "matching" : "different";
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
