// Which tab an AI agent works in: the one the user has open for that server
// (the active one first, then the most recently used), otherwise a new tab
// opened in the background. The agent waits for it to connect — including
// any host-key decision the user has to make — and, for commands, for its
// terminal.

import type { AgentTabInfo } from "$lib/app/contracts/api";

export type AgentTabState = "connecting" | "connected" | "error" | "disconnected";

export interface AgentTabSnapshot {
  tabId: string;
  connectionId: string;
  sessionId: string | null;
  state: AgentTabState;
  error: string | null;
  active: boolean;
}

/** The session tabs as the agent needs them. */
export interface AgentTabsPort {
  list(): AgentTabSnapshot[];
  /** Open a tab without switching to it; returns its id. */
  open(connectionId: string): string;
  reconnect(tabId: string): void;
}

/** The terminal registry fed by the terminal views. */
export interface AgentTerminalLookup {
  activeTerminal(sessionId: string): string | null;
  /** Ask the tab to open a terminal when it has none. */
  requestTerminal(sessionId: string): void;
}

export interface EnsureTabTimeouts {
  connectMs: number;
  terminalMs: number;
}

/** Together below the backend's 180 s, so the UI never answers a request
 *  the backend already gave up on. */
export const DEFAULT_TIMEOUTS: EnsureTabTimeouts = { connectMs: 155_000, terminalMs: 20_000 };

/** Poll `check` until it yields a value. `check` may throw to give up. */
export async function waitFor<T>(
  check: () => T | null | undefined,
  timeoutMs: number,
  what: string,
  intervalMs = 100,
): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = check();
    if (value !== null && value !== undefined) return value;
    if (Date.now() >= deadline) throw new Error(`Timed out waiting for ${what}.`);
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
}

/** The best existing tab for a server, if any. */
export function pickTab(
  tabs: AgentTabSnapshot[],
  connectionId: string,
  recency: ReadonlyMap<string, number>,
): AgentTabSnapshot | null {
  const candidates = tabs.filter((tab) => tab.connectionId === connectionId);
  const active = candidates.find((tab) => tab.active);
  if (active) return active;
  // Prefer live tabs, then the one the user looked at last.
  const score = (tab: AgentTabSnapshot) =>
    (tab.state === "connected" ? 1e15 : 0) + (recency.get(tab.tabId) ?? 0);
  return candidates.reduce<AgentTabSnapshot | null>(
    (best, tab) => (best === null || score(tab) > score(best) ? tab : best),
    null,
  );
}

export async function ensureTab(
  port: AgentTabsPort,
  terminals: AgentTerminalLookup,
  recency: ReadonlyMap<string, number>,
  connectionId: string | null,
  needTerminal: boolean,
  timeouts: EnsureTabTimeouts = DEFAULT_TIMEOUTS,
): Promise<AgentTabInfo> {
  let tabId: string;
  if (connectionId === null) {
    const active = port.list().find((tab) => tab.active);
    if (!active) throw new Error("No tab is active in Serverus.");
    tabId = active.tabId;
  } else {
    const existing = pickTab(port.list(), connectionId, recency);
    if (!existing) {
      tabId = port.open(connectionId);
    } else {
      tabId = existing.tabId;
      if (existing.state === "error" || existing.state === "disconnected") {
        port.reconnect(tabId);
      }
    }
  }

  const connected = await waitFor(
    () => {
      const tab = port.list().find((candidate) => candidate.tabId === tabId);
      if (!tab) throw new Error("The tab was closed.");
      if (tab.state === "error") throw new Error(tab.error ?? "The connection failed.");
      return tab.state === "connected" && tab.sessionId ? tab : null;
    },
    timeouts.connectMs,
    "the server to connect",
  );
  const sessionId = connected.sessionId as string;

  let termId: string | null = terminals.activeTerminal(sessionId);
  if (needTerminal && !termId) {
    terminals.requestTerminal(sessionId);
    termId = await waitFor(
      () => terminals.activeTerminal(sessionId),
      timeouts.terminalMs,
      "the terminal to open",
    );
  }
  return {
    tab_id: tabId,
    connection_id: connected.connectionId,
    session_id: sessionId,
    state: connected.state,
    active: connected.active,
    term_id: termId,
  };
}
