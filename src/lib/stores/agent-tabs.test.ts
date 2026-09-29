import { describe, expect, it, vi } from "vitest";
import {
  ensureTab,
  pickTab,
  type AgentTabSnapshot,
  type AgentTabsPort,
  type AgentTerminalLookup,
} from "./agent-tabs";

const fast = { connectMs: 500, terminalMs: 500 };

function tab(tabId: string, connectionId: string, extra: Partial<AgentTabSnapshot> = {}): AgentTabSnapshot {
  return {
    tabId,
    connectionId,
    sessionId: `session-${tabId}`,
    state: "connected",
    error: null,
    active: false,
    ...extra,
  };
}

class FakeTabs implements AgentTabsPort {
  opened: string[] = [];
  reconnected: string[] = [];
  constructor(public tabs: AgentTabSnapshot[]) {}
  list() {
    return this.tabs.map((t) => ({ ...t }));
  }
  open(connectionId: string) {
    const id = `new-${this.opened.length + 1}`;
    this.opened.push(connectionId);
    this.tabs.push(tab(id, connectionId, { sessionId: null, state: "connecting" }));
    return id;
  }
  reconnect(tabId: string) {
    this.reconnected.push(tabId);
    const target = this.tabs.find((t) => t.tabId === tabId)!;
    target.state = "connecting";
  }
  update(tabId: string, change: Partial<AgentTabSnapshot>) {
    Object.assign(this.tabs.find((t) => t.tabId === tabId)!, change);
  }
}

function terminals(map: Record<string, string> = {}): AgentTerminalLookup & { requested: string[] } {
  const requested: string[] = [];
  return {
    requested,
    activeTerminal: (sessionId) => map[sessionId] ?? null,
    requestTerminal: (sessionId) => {
      requested.push(sessionId);
      map[sessionId] = `term-of-${sessionId}`;
    },
  };
}

describe("pickTab", () => {
  it("prefers the active tab, then live and recently used ones", () => {
    const tabs = [
      tab("a", "c1", { state: "error" }),
      tab("b", "c1"),
      tab("c", "c1"),
      tab("d", "c2", { active: true }),
    ];
    const recency = new Map([
      ["a", 9],
      ["b", 1],
      ["c", 2],
    ]);
    expect(pickTab(tabs, "c1", recency)?.tabId).toBe("c");
    expect(pickTab(tabs, "c2", recency)?.tabId).toBe("d");
    expect(pickTab(tabs, "c3", recency)).toBeNull();
  });
});

describe("ensureTab", () => {
  it("reuses the user's open tab and its active terminal", async () => {
    const port = new FakeTabs([tab("a", "c1", { active: true })]);
    const info = await ensureTab(port, terminals({ "session-a": "t1" }), new Map(), "c1", true, fast);
    expect(info).toMatchObject({ tab_id: "a", session_id: "session-a", term_id: "t1" });
    expect(port.opened).toEqual([]);
  });

  it("opens a background tab and waits for it to connect", async () => {
    const port = new FakeTabs([tab("a", "other", { active: true })]);
    setTimeout(() => port.update("new-1", { state: "connected", sessionId: "s-new" }), 30);
    const info = await ensureTab(port, terminals({ "s-new": "t9" }), new Map(), "c1", false, fast);
    expect(port.opened).toEqual(["c1"]);
    expect(info).toMatchObject({ tab_id: "new-1", session_id: "s-new" });
  });

  it("reconnects a failed tab and reports a failed connection", async () => {
    const port = new FakeTabs([tab("a", "c1", { state: "error", error: "old" })]);
    setTimeout(() => port.update("a", { state: "error", error: "auth failed" }), 30);
    await expect(ensureTab(port, terminals(), new Map(), "c1", false, fast)).rejects.toThrow(
      "auth failed",
    );
    expect(port.reconnected).toEqual(["a"]);
  });

  it("asks for a terminal when the tab has none", async () => {
    const port = new FakeTabs([tab("a", "c1")]);
    const lookup = terminals();
    const info = await ensureTab(port, lookup, new Map(), "c1", true, fast);
    expect(lookup.requested).toEqual(["session-a"]);
    expect(info.term_id).toBe("term-of-session-a");
  });

  it("uses the active tab for the current server and needs one", async () => {
    const port = new FakeTabs([tab("a", "c1"), tab("b", "c2", { active: true })]);
    const info = await ensureTab(port, terminals(), new Map(), null, false, fast);
    expect(info.connection_id).toBe("c2");
    port.tabs.forEach((t) => (t.active = false));
    await expect(ensureTab(port, terminals(), new Map(), null, false, fast)).rejects.toThrow(
      "No tab is active",
    );
  });

  it("gives up when the tab is closed while connecting", async () => {
    vi.useRealTimers();
    const port = new FakeTabs([tab("a", "c1", { state: "connecting", sessionId: null })]);
    setTimeout(() => (port.tabs = []), 30);
    await expect(ensureTab(port, terminals(), new Map(), "c1", false, fast)).rejects.toThrow(
      "closed",
    );
  });
});
