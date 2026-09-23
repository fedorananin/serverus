// Remote deletes and recursive chmods run as items of the session's transfer
// panel. This wires a remote pane to them: the actions the pane invokes, and
// an effect that dims rows being deleted and relists the pane — or steps out
// of a folder that is gone — as each operation finishes, without waiting for
// the whole transfer queue to settle.

import type { ChmodScope, RemoteEntry } from "$lib/api";
import { deletingPaths, followUp, TreeOpTracker } from "$lib/tree-ops";
import type { PaneController } from "./pane.svelte";
import type { TransfersStore } from "./transfers.svelte";

export interface RemoteTreeActions {
  delete(entries: RemoteEntry[]): Promise<void>;
  chmod(entry: RemoteEntry, mode: number, scope: ChmodScope): Promise<void>;
}

/** Must be called during component initialisation (owns an effect). */
export function trackRemoteTreeOps(
  pane: PaneController,
  transfers: TransfersStore,
  sessionId: string,
): RemoteTreeActions {
  const tracker = new TreeOpTracker();
  $effect(() => {
    const items = transfers.itemsFor(sessionId);
    pane.deleting = deletingPaths(items);
    const steps = tracker.finished(items).map((item) => followUp(pane.path, item));
    const out = steps.find((step) => step?.kind === "navigate");
    if (out?.kind === "navigate") void pane.navigate(out.path);
    else if (steps.some((step) => step?.kind === "refresh")) void pane.refresh();
  });
  return {
    // A symlink is removed as a link, never walked into.
    delete: (entries) =>
      transfers.delete(
        sessionId,
        entries.map((entry) => ({ path: entry.path, is_dir: entry.is_dir && !entry.is_symlink })),
      ),
    chmod: (entry, mode, scope) =>
      transfers.chmod(sessionId, [{ path: entry.path, is_dir: true }], mode, scope),
  };
}
