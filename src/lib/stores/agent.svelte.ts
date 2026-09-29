// AI agent (MCP) state on the UI side: answers the backend's requests
// (which tabs are open, open one, ask the user to confirm), tracks which
// terminal each tab shows, who controls each terminal, and the activity
// journal shown per tab.

import type {
  AgentActivityEntry,
  AgentApi,
  AgentConfirmDecision,
  AgentTabInfo,
  AgentTerminalEvent,
  AgentUiRequest,
  AgentUiResponse,
} from "$lib/app/contracts/api";
import type {
  AgentActivityEvent,
  AgentEventSource,
  AgentFsChangedEvent,
  AgentUiRequestEvent,
  AgentUiRequestExpiredEvent,
  AgentVaultChangedEvent,
  AppUnlisten,
} from "$lib/app/contracts/events";
import { ensureTab, type AgentTabsPort, type EnsureTabTimeouts } from "./agent-tabs";

/** Journal entries kept per session. */
const ACTIVITY_LIMIT = 200;

/** The rest of the app, as the agent store needs it. */
export interface AgentHostPort {
  /** The agent changed the vault: show the new (secret-free) contents. */
  applyVault(event: AgentVaultChangedEvent): void;
  /** Tell the user something in passing (a toast). */
  notify(message: string): void;
}

export interface PendingConfirmation {
  requestId: string;
  connectionId: string;
  server: string;
  action: string;
  detail: string;
}

interface SessionTerminals {
  active: string | null;
  all: string[];
}

function message(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && "message" in error) {
    return String((error as { message: unknown }).message);
  }
  return String(error);
}

export class AgentStore {
  /** Confirmations waiting for the user, oldest first. */
  confirmations = $state<PendingConfirmation[]>([]);
  /** Agent state per terminal id. */
  terminals = $state<Record<string, AgentTerminalEvent>>({});
  /** Journal per session id, newest first. */
  activity = $state<Record<string, AgentActivityEntry[]>>({});
  /** Per session: bumped when the agent changed remote paths directly. */
  fsChanges = $state<Record<string, { revision: number; paths: string[] }>>({});
  /** Per session: bumped when the agent needs a terminal the tab lacks. */
  terminalRequests = $state<Record<string, number>>({});
  private sessionTerminals = $state<Record<string, SessionTerminals>>({});
  private readonly recency = new Map<string, number>();
  private recencyTick = 0;
  private tabs: AgentTabsPort | null = null;
  private host: AgentHostPort | null = null;

  constructor(
    private readonly api: AgentApi,
    private readonly events: AgentEventSource,
    private readonly timeouts?: EnsureTabTimeouts,
  ) {}

  /** Start serving the backend's UI requests; returns the cleanup. */
  start(tabs: AgentTabsPort, host?: AgentHostPort): () => void {
    this.tabs = tabs;
    this.host = host ?? null;
    let disposed = false;
    const unlisteners: AppUnlisten[] = [];
    const keep = (subscription: Promise<AppUnlisten>) =>
      void subscription.then((unlisten) => (disposed ? unlisten() : unlisteners.push(unlisten)));
    keep(this.events.listenUiRequests((event) => void this.handle(event)));
    keep(this.events.listenUiExpired((event) => this.expire(event)));
    keep(this.events.listenTerminal((event) => this.applyTerminal(event)));
    keep(this.events.listenActivity((event) => this.applyActivity(event)));
    keep(this.events.listenFsChanged((event) => this.applyFsChange(event)));
    keep(this.events.listenVaultChanged((event) => host?.applyVault(event)));
    void this.api
      .terminalStates()
      .then((states) => states.forEach((state) => this.applyTerminal(state)))
      .catch(() => {});
    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
      if (this.tabs === tabs) {
        this.tabs = null;
        this.host = null;
      }
    };
  }

  noteTabActivated(tabId: string) {
    this.recency.set(tabId, ++this.recencyTick);
  }

  // Terminal registry, fed by the terminal views.

  terminalOpened(sessionId: string, termId: string) {
    const entry = this.sessionTerminals[sessionId] ?? { active: null, all: [] };
    this.sessionTerminals[sessionId] = {
      active: entry.active ?? termId,
      all: [...entry.all.filter((id) => id !== termId), termId],
    };
  }

  terminalActivated(sessionId: string, termId: string) {
    const entry = this.sessionTerminals[sessionId];
    if (entry?.all.includes(termId)) this.sessionTerminals[sessionId] = { ...entry, active: termId };
  }

  terminalClosed(sessionId: string, termId: string) {
    const entry = this.sessionTerminals[sessionId];
    if (!entry) return;
    const all = entry.all.filter((id) => id !== termId);
    const active = entry.active === termId ? (all.at(-1) ?? null) : entry.active;
    this.sessionTerminals[sessionId] = { active, all };
    delete this.terminals[termId];
  }

  activeTerminal(sessionId: string): string | null {
    return this.sessionTerminals[sessionId]?.active ?? null;
  }

  requestTerminal(sessionId: string) {
    this.terminalRequests[sessionId] = (this.terminalRequests[sessionId] ?? 0) + 1;
  }

  // Views.

  terminal(termId: string | null): AgentTerminalEvent | null {
    return termId ? (this.terminals[termId] ?? null) : null;
  }

  activityFor(sessionId: string | null): AgentActivityEntry[] {
    return sessionId ? (this.activity[sessionId] ?? []) : [];
  }

  /** Whether the agent is doing something in the session right now. */
  isBusy(sessionId: string | null): boolean {
    if (!sessionId) return false;
    return (
      this.activityFor(sessionId).some((entry) => entry.status === "running") ||
      Object.values(this.terminals).some((t) => t.session_id === sessionId && t.running)
    );
  }

  // User intents.

  async decide(requestId: string, decision: AgentConfirmDecision) {
    this.confirmations = this.confirmations.filter((pending) => pending.requestId !== requestId);
    const delivered = await this.api.respond(requestId, { kind: "confirm", decision }).catch(() => false);
    // Normally the expiry event removed the dialog first; this is the race.
    if (!delivered && decision !== "deny") {
      this.host?.notify("That AI agent request had already expired — nothing was done.");
    }
  }

  takeOver(termId: string) {
    return this.api.takeOver(termId);
  }

  handBack(termId: string) {
    return this.api.handBack(termId);
  }

  // Backend events.

  private async handle(event: AgentUiRequestEvent) {
    let response: AgentUiResponse | null;
    try {
      response = await this.resolve(event.request_id, event.request);
    } catch (error) {
      response = { kind: "error", message: message(error) };
    }
    if (response) await this.api.respond(event.request_id, response).catch(() => false);
  }

  private async resolve(requestId: string, request: AgentUiRequest): Promise<AgentUiResponse | null> {
    const tabs = this.tabs;
    if (!tabs) throw new Error("Serverus is not ready yet.");
    switch (request.kind) {
      case "tabs":
        return { kind: "tabs", tabs: this.tabInfos(tabs) };
      case "open_tab": {
        const tab = await ensureTab(
          tabs,
          this,
          this.recency,
          request.connection_id,
          request.need_terminal,
          this.timeouts,
        );
        return { kind: "tab", tab };
      }
      case "confirm":
        // Answered later, from the dialog, through decide().
        this.confirmations.push({
          requestId,
          connectionId: request.connection_id,
          server: request.server,
          action: request.action,
          detail: request.detail,
        });
        return null;
    }
  }

  private tabInfos(tabs: AgentTabsPort): AgentTabInfo[] {
    return tabs.list().map((tab) => ({
      tab_id: tab.tabId,
      connection_id: tab.connectionId,
      session_id: tab.sessionId,
      state: tab.state,
      active: tab.active,
      term_id: tab.sessionId ? this.activeTerminal(tab.sessionId) : null,
    }));
  }

  private expire(event: AgentUiRequestExpiredEvent) {
    this.confirmations = this.confirmations.filter((pending) => pending.requestId !== event.request_id);
  }

  private applyTerminal(event: AgentTerminalEvent) {
    this.terminals[event.term_id] = event;
  }

  private applyActivity(event: AgentActivityEvent) {
    if (!event.session_id) return;
    const entries = this.activity[event.session_id] ?? [];
    const index = entries.findIndex((entry) => entry.id === event.entry.id);
    this.activity[event.session_id] =
      index === -1
        ? [event.entry, ...entries].slice(0, ACTIVITY_LIMIT)
        : entries.map((entry, i) => (i === index ? event.entry : entry));
  }

  private applyFsChange(event: AgentFsChangedEvent) {
    const revision = (this.fsChanges[event.session_id]?.revision ?? 0) + 1;
    this.fsChanges[event.session_id] = { revision, paths: event.paths };
  }
}
