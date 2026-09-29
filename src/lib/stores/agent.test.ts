import { describe, expect, it, vi } from "vitest";
import type { AgentApi, AgentUiResponse } from "$lib/app/contracts/api";
import type {
  AgentActivityEvent,
  AgentEventSource,
  AgentFsChangedEvent,
  AgentUiRequestEvent,
  AgentUiRequestExpiredEvent,
  AgentVaultChangedEvent,
} from "$lib/app/contracts/events";
import type { AgentTerminalEvent } from "$lib/api";
import { AgentStore } from "./agent.svelte";
import type { AgentTabsPort } from "./agent-tabs";

class FakeEvents implements AgentEventSource {
  ui: ((event: AgentUiRequestEvent) => void) | null = null;
  expired: ((event: AgentUiRequestExpiredEvent) => void) | null = null;
  terminal: ((event: AgentTerminalEvent) => void) | null = null;
  activity: ((event: AgentActivityEvent) => void) | null = null;
  fs: ((event: AgentFsChangedEvent) => void) | null = null;
  vault: ((event: AgentVaultChangedEvent) => void) | null = null;
  listenUiRequests = async (listener: (event: AgentUiRequestEvent) => void) => {
    this.ui = listener;
    return () => (this.ui = null);
  };
  listenUiExpired = async (listener: (event: AgentUiRequestExpiredEvent) => void) => {
    this.expired = listener;
    return () => (this.expired = null);
  };
  listenTerminal = async (listener: (event: AgentTerminalEvent) => void) => {
    this.terminal = listener;
    return () => (this.terminal = null);
  };
  listenActivity = async (listener: (event: AgentActivityEvent) => void) => {
    this.activity = listener;
    return () => (this.activity = null);
  };
  listenFsChanged = async (listener: (event: AgentFsChangedEvent) => void) => {
    this.fs = listener;
    return () => (this.fs = null);
  };
  listenVaultChanged = async (listener: (event: AgentVaultChangedEvent) => void) => {
    this.vault = listener;
    return () => (this.vault = null);
  };
}

function fakeApi() {
  const responses: Array<[string, AgentUiResponse]> = [];
  const expired = new Set<string>();
  const api: AgentApi = {
    respond: vi.fn(async (id: string, response: AgentUiResponse) => {
      responses.push([id, response]);
      return !expired.has(id);
    }),
    takeOver: vi.fn(async () => {}),
    handBack: vi.fn(async () => {}),
    terminalStates: vi.fn(async () => [
      { term_id: "t0", session_id: "s1", running: null, user_control: true },
    ]),
    setupInfo: vi.fn(),
  };
  return { api, responses, expired };
}

const tabs: AgentTabsPort = {
  list: () => [
    { tabId: "a", connectionId: "c1", sessionId: "s1", state: "connected", error: null, active: true },
  ],
  open: () => "never",
  reconnect: () => {},
};

async function started() {
  const events = new FakeEvents();
  const { api, responses, expired } = fakeApi();
  const store = new AgentStore(api, events, { connectMs: 300, terminalMs: 300 });
  const applied: AgentVaultChangedEvent[] = [];
  const notices: string[] = [];
  const stop = store.start(tabs, {
    applyVault: (event) => applied.push(event),
    notify: (message) => notices.push(message),
  });
  await vi.waitFor(() => expect(events.ui).not.toBeNull());
  await vi.waitFor(() => expect(events.expired).not.toBeNull());
  return { store, events, api, responses, expired, stop, applied, notices };
}

function activity(id: string, status: "running" | "done", session = "s1"): AgentActivityEvent {
  return {
    session_id: session,
    connection_id: "c1",
    entry: { id, tool: "run_command", summary: "ls", status, detail: null, at_ms: 1 },
  };
}

describe("AgentStore", () => {
  it("answers tab queries with each tab's active terminal", async () => {
    const { store, events, responses } = await started();
    store.terminalOpened("s1", "t1");
    events.ui!({ request_id: "r1", request: { kind: "tabs" } });
    await vi.waitFor(() => expect(responses).toHaveLength(1));
    expect(responses[0]).toEqual([
      "r1",
      {
        kind: "tabs",
        tabs: [
          { tab_id: "a", connection_id: "c1", session_id: "s1", state: "connected", active: true, term_id: "t1" },
        ],
      },
    ]);
  });

  it("queues confirmations until the user decides", async () => {
    const { store, events, responses } = await started();
    const confirm = (id: string) =>
      events.ui!({
        request_id: id,
        request: { kind: "confirm", connection_id: "c1", server: "web", action: "Run", detail: "ls" },
      });
    confirm("r1");
    confirm("r2");
    await vi.waitFor(() => expect(store.confirmations).toHaveLength(2));
    expect(responses).toEqual([]);

    await store.decide("r1", "for_a_while");
    expect(store.confirmations.map((c) => c.requestId)).toEqual(["r2"]);
    expect(responses).toEqual([["r1", { kind: "confirm", decision: "for_a_while" }]]);
  });

  it("drops a confirmation the backend gave up on", async () => {
    const { store, events } = await started();
    events.ui!({
      request_id: "r1",
      request: { kind: "confirm", connection_id: "c1", server: "web", action: "Run", detail: "ls" },
    });
    await vi.waitFor(() => expect(store.confirmations).toHaveLength(1));
    events.expired!({ request_id: "r1" });
    expect(store.confirmations).toEqual([]);
  });

  it("tells the user when an approval arrived too late", async () => {
    const { store, events, expired, notices } = await started();
    events.ui!({
      request_id: "r1",
      request: { kind: "confirm", connection_id: "c1", server: "web", action: "Run", detail: "ls" },
    });
    await vi.waitFor(() => expect(store.confirmations).toHaveLength(1));
    expired.add("r1");
    await store.decide("r1", "once");
    expect(notices).toHaveLength(1);
    // A late "deny" changes nothing either way: no notice.
    await store.decide("r1", "deny");
    expect(notices).toHaveLength(1);
  });

  it("reports a failed tab request as an error answer", async () => {
    const { events, responses } = await started();
    events.ui!({ request_id: "r1", request: { kind: "open_tab", connection_id: "c1", need_terminal: true } });
    await vi.waitFor(() => expect(responses).toHaveLength(1), { timeout: 2000 });
    expect(responses[0][1]).toMatchObject({ kind: "error" });
  });

  it("tracks the active terminal of each session", async () => {
    const { store } = await started();
    store.terminalOpened("s1", "t1");
    store.terminalOpened("s1", "t2");
    expect(store.activeTerminal("s1")).toBe("t1");
    store.terminalActivated("s1", "t2");
    expect(store.activeTerminal("s1")).toBe("t2");
    store.terminalClosed("s1", "t2");
    expect(store.activeTerminal("s1")).toBe("t1");
    store.terminalActivated("s1", "unknown");
    expect(store.activeTerminal("s1")).toBe("t1");
  });

  it("keeps the journal per session and knows when the agent is busy", async () => {
    const { store, events } = await started();
    events.activity!(activity("e1", "running"));
    expect(store.isBusy("s1")).toBe(true);
    events.activity!(activity("e1", "done"));
    events.activity!(activity("e2", "done"));
    expect(store.activityFor("s1").map((e) => [e.id, e.status])).toEqual([
      ["e2", "done"],
      ["e1", "done"],
    ]);
    expect(store.isBusy("s1")).toBe(false);
    events.terminal!({ term_id: "t1", session_id: "s1", running: "make", user_control: false });
    expect(store.isBusy("s1")).toBe(true);
    expect(store.isBusy("s2")).toBe(false);
  });

  it("restores terminal states and counts file-system changes", async () => {
    const { store, events } = await started();
    await vi.waitFor(() => expect(store.terminal("t0")?.user_control).toBe(true));
    events.fs!({ session_id: "s1", paths: ["/a"] });
    events.fs!({ session_id: "s1", paths: ["/b"] });
    expect(store.fsChanges.s1).toEqual({ revision: 2, paths: ["/b"] });
  });

  it("stops listening when stopped", async () => {
    const { events, stop } = await started();
    stop();
    expect(events.ui).toBeNull();
  });

  it("hands vault changes made by the agent to the app", async () => {
    const { events, applied } = await started();
    await vi.waitFor(() => expect(events.vault).not.toBeNull());
    const vault = { tree: [], connections: {}, known_hosts: {}, settings: {} };
    events.vault!({ vault, summary: "added" } as unknown as AgentVaultChangedEvent);
    expect(applied.map((event) => event.summary)).toEqual(["added"]);
  });
});
