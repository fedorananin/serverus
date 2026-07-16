// Open tabs — one tab per server session (SPEC §5.1). Each tab owns one
// backend session; the same server can be open in two tabs (two sessions).

import { commands, errorMessage, events, isApiError, unwrap } from "$lib/api";
import { hostKey } from "./hostkey.svelte";
import { vault } from "./vault.svelte";

export type TabView = "files" | "terminal" | "tunnels";
export type TabState = "connecting" | "connected" | "error" | "disconnected";

export interface Tab {
  /** Unique tab id (not the connection id). */
  id: string;
  connectionId: string;
  view: TabView;
  sessionId: string | null;
  state: TabState;
  error: string | null;
  /** Current connect stage ("Authenticating…") streamed from the backend. */
  connectMessage: string | null;
  /** Last remote dir, restored after reconnect (SPEC §4.1). */
  lastRemoteDir: string | null;
  /** Consecutive automatic reconnect attempts. */
  reconnectAttempts: number;
}

let nextId = 1;

class TabsStore {
  tabs = $state<Tab[]>([]);
  activeId = $state<string | null>(null);
  private listening = false;
  /** Latest in-flight connect call for each tab. Older results are stale. */
  private connectAttempts = new Map<string, symbol>();

  get active(): Tab | null {
    return this.tabs.find((t) => t.id === this.activeId) ?? null;
  }

  /** Watch session lifecycle events: connect-stage messages for the
   *  progress indicator, and drops for auto-reconnect (SPEC §4.1). */
  private async listenForDisconnects() {
    if (this.listening) return;
    this.listening = true;
    await events.sessionStateEvent.listen((e) => {
      if (e.payload.state === "connecting" && e.payload.message) {
        // The session id isn't known to the tab until connect returns —
        // match by connection instead (a same-server twin tab is harmless).
        for (const t of this.tabs) {
          if (t.connectionId === e.payload.connection_id && t.state === "connecting") {
            t.connectMessage = e.payload.message;
          }
        }
        return;
      }
      if (e.payload.state !== "disconnected") return;
      const tab = this.tabs.find((t) => t.sessionId === e.payload.session_id);
      if (!tab) return;
      tab.state = "disconnected";
      tab.sessionId = null;
      if (tab.reconnectAttempts < 3) {
        tab.reconnectAttempts += 1;
        const delay = 1000 * tab.reconnectAttempts;
        setTimeout(() => {
          if (tab.state === "disconnected") void this.connect(tab.id);
        }, delay);
      } else {
        tab.state = "error";
        tab.error = "Connection lost — automatic reconnect failed";
      }
    });
  }

  open(connectionId: string) {
    void this.listenForDisconnects();
    const conn = vault.data?.connections[connectionId];
    // Terminal-less servers (FTP, S3, or SSH with the shell disabled) open on Files.
    const startOnFiles =
      conn?.protocol === "ftp" || conn?.protocol === "s3" || conn?.disable_terminal;
    const tab: Tab = {
      id: `tab-${nextId++}`,
      connectionId,
      view: startOnFiles ? "files" : "terminal",
      sessionId: null,
      state: "connecting",
      error: null,
      connectMessage: null,
      lastRemoteDir: null,
      reconnectAttempts: 0,
    };
    this.tabs.push(tab);
    this.activeId = tab.id;
    void this.connect(tab.id);
    return tab;
  }

  /** (Re)connect the tab's backend session. */
  async connect(tabId: string) {
    const tab = this.tabs.find((t) => t.id === tabId);
    if (!tab) return;
    const attempt = Symbol(tabId);
    const accessGeneration = vault.accessGeneration;
    this.connectAttempts.set(tabId, attempt);
    const isCurrent = () =>
      this.tabs.includes(tab) && this.connectAttempts.get(tabId) === attempt;
    tab.state = "connecting";
    tab.error = null;
    tab.connectMessage = null;
    try {
      const dto = await unwrap(commands.sessionConnect(tab.connectionId));
      if (!isCurrent()) {
        // A closed tab or a newer retry cannot own this backend session.
        await unwrap(commands.sessionDisconnect(dto.session_id)).catch(() => {});
        return;
      }
      this.connectAttempts.delete(tabId);
      tab.sessionId = dto.session_id;
      tab.state = "connected";
      tab.connectMessage = null;
      tab.reconnectAttempts = 0;
    } catch (e) {
      // Errors and prompts also belong only to the latest live attempt.
      if (!isCurrent()) return;
      this.connectAttempts.delete(tabId);
      if (isApiError(e) && e.code === "host_key_prompt" && e.host_key) {
        if (!vault.isAccessCurrent(accessGeneration)) {
          tab.state = "error";
          tab.error = "Vault access was revoked";
          return;
        }
        hostKey.ask(e.host_key, {
          accepted: () => void this.connect(tabId),
          rejected: () => {
            tab.state = "error";
            tab.error = "Host key rejected";
          },
        });
      } else {
        tab.state = "error";
        tab.error = errorMessage(e);
      }
    }
  }

  close(id: string) {
    const idx = this.tabs.findIndex((t) => t.id === id);
    if (idx === -1) return;
    const [tab] = this.tabs.splice(idx, 1);
    this.connectAttempts.delete(id);
    if (tab.sessionId) {
      void unwrap(commands.sessionDisconnect(tab.sessionId)).catch(() => {});
    }
    if (this.activeId === id) {
      this.activeId = this.tabs[Math.min(idx, this.tabs.length - 1)]?.id ?? null;
    }
  }

  /** Drop all UI ownership of a retired backend runtime context. In-flight
   *  connects remain responsible for disconnecting any success that arrives
   *  after this reset, but they can no longer register it on an old tab. */
  retireContext() {
    this.connectAttempts.clear();
    this.tabs = [];
    this.activeId = null;
  }

  activate(id: string) {
    if (this.tabs.some((t) => t.id === id)) this.activeId = id;
  }

  /** Move a tab to a new position in the strip (drag reorder / ⌘⇧←→). */
  move(id: string, toIndex: number) {
    const from = this.tabs.findIndex((t) => t.id === id);
    if (from === -1) return;
    const to = Math.max(0, Math.min(toIndex, this.tabs.length - 1));
    if (to === from) return;
    const [tab] = this.tabs.splice(from, 1);
    this.tabs.splice(to, 0, tab);
  }

  activateIndex(i: number) {
    const tab = this.tabs[i];
    if (tab) this.activeId = tab.id;
  }
}

export const tabs = new TabsStore();
