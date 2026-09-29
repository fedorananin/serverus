// Relist a remote pane when the AI agent changed paths in the directory it
// shows outside the transfer queue (mkdir, rename, chmod) — queued work
// already refreshes panes when the queue settles.

import type { AgentStore } from "./agent.svelte";
import type { PaneController } from "./pane.svelte";

function trimmed(path: string): string {
  const stripped = path.replace(/\/+$/u, "");
  return stripped === "" ? "/" : stripped;
}

function parentOf(path: string): string {
  const clean = trimmed(path);
  const index = clean.lastIndexOf("/");
  return index <= 0 ? "/" : clean.slice(0, index);
}

/** Whether a pane showing `shown` must relist after `changed` changed. */
export function paneAffected(shown: string, changed: string): boolean {
  const pane = trimmed(shown);
  return trimmed(changed) === pane || parentOf(changed) === pane;
}

/** Must be called during component initialisation (owns an effect). */
export function trackAgentFsChanges(pane: PaneController, agent: AgentStore, sessionId: string) {
  let seen = agent.fsChanges[sessionId]?.revision ?? 0;
  $effect(() => {
    const change = agent.fsChanges[sessionId];
    if (!change || change.revision === seen) return;
    seen = change.revision;
    if (change.paths.some((path) => paneAffected(pane.path, path))) void pane.refresh();
  });
}
