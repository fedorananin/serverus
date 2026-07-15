import { beforeEach, describe, expect, it, vi } from "vitest";
import { tabs, type Tab } from "./tabs.svelte";
const apiMocks = vi.hoisted(() => ({
  sessionConnect: vi.fn(),
  sessionDisconnect: vi.fn(async () => ({ status: "ok", data: null })),
}));
const hostKeyMocks = vi.hoisted(() => ({
  ask: vi.fn(),
}));
const vaultMocks = vi.hoisted(() => ({
  accessGeneration: 1,
  data: {} as object | null,
  isAccessCurrent(generation: number) {
    return this.data !== null && this.accessGeneration === generation;
  },
}));

vi.mock("$lib/api", () => ({
  commands: {
    sessionConnect: apiMocks.sessionConnect,
    sessionDisconnect: apiMocks.sessionDisconnect,
  },
  events: {
    sessionStateEvent: { listen: vi.fn(async () => () => {}) },
  },
  errorMessage: (error: unknown) =>
    typeof error === "object" && error !== null && "message" in error
      ? String(error.message)
      : String(error),
  isApiError: (error: unknown) =>
    typeof error === "object" && error !== null && "code" in error && "message" in error,
  unwrap: async (promise: Promise<{ status: "ok"; data: unknown } | { status: "error"; error: unknown }>) => {
    const result = await promise;
    if (result.status === "error") throw result.error;
    return result.data;
  },
}));

vi.mock("./hostkey.svelte", () => ({
  hostKey: { ask: hostKeyMocks.ask },
}));

vi.mock("./vault.svelte", () => ({
  vault: vaultMocks,
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function createTab(): Tab {
  return {
    id: "tab-a",
    connectionId: "connection-a",
    view: "terminal",
    sessionId: null,
    state: "connecting",
    error: null,
    connectMessage: null,
    lastRemoteDir: null,
    reconnectAttempts: 0,
  };
}

describe("TabsStore connection attempts", () => {
  beforeEach(() => {
    tabs.tabs = [];
    tabs.activeId = null;
    apiMocks.sessionConnect.mockReset();
    apiMocks.sessionDisconnect.mockClear();
    hostKeyMocks.ask.mockReset();
    vaultMocks.accessGeneration = 1;
    vaultMocks.data = {};
  });

  it("disconnects a session that finishes after its tab closes", async () => {
    const pending = deferred<{
      status: "ok";
      data: { session_id: string; connection_id: string };
    }>();
    apiMocks.sessionConnect.mockReturnValue(pending.promise);
    const tab = createTab();
    tabs.tabs = [tab];
    tabs.activeId = tab.id;

    const connecting = tabs.connect(tab.id);
    tabs.close(tab.id);
    pending.resolve({
      status: "ok",
      data: { session_id: "late-session", connection_id: tab.connectionId },
    });
    await connecting;

    expect(tabs.tabs).toHaveLength(0);
    expect(apiMocks.sessionDisconnect).toHaveBeenCalledWith("late-session");
  });

  it("clears every old tab when its runtime context retires", () => {
    const first = {
      ...createTab(),
      sessionId: "session-a",
      state: "connected" as const,
    };
    const second = {
      ...createTab(),
      id: "tab-b",
      connectionId: "connection-b",
    };
    tabs.tabs = [first, second];
    tabs.activeId = second.id;

    tabs.retireContext();

    expect(tabs.tabs).toEqual([]);
    expect(tabs.activeId).toBeNull();
  });

  it("disconnects a late success after its runtime context retires", async () => {
    const pending = deferred<{
      status: "ok";
      data: { session_id: string; connection_id: string };
    }>();
    apiMocks.sessionConnect.mockReturnValue(pending.promise);
    const tab = createTab();
    tabs.tabs = [tab];
    tabs.activeId = tab.id;

    const connecting = tabs.connect(tab.id);
    tabs.retireContext();
    pending.resolve({
      status: "ok",
      data: { session_id: "late-retired-session", connection_id: tab.connectionId },
    });
    await connecting;

    expect(tabs.tabs).toEqual([]);
    expect(tabs.activeId).toBeNull();
    expect(apiMocks.sessionDisconnect).toHaveBeenCalledWith("late-retired-session");
  });

  it("disconnects an older success instead of replacing the latest session", async () => {
    const first = deferred<{
      status: "ok";
      data: { session_id: string; connection_id: string };
    }>();
    const second = deferred<{
      status: "ok";
      data: { session_id: string; connection_id: string };
    }>();
    apiMocks.sessionConnect
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const tab = createTab();
    tabs.tabs = [tab];

    const firstConnect = tabs.connect(tab.id);
    const secondConnect = tabs.connect(tab.id);
    second.resolve({
      status: "ok",
      data: { session_id: "latest-session", connection_id: tab.connectionId },
    });
    await secondConnect;
    first.resolve({
      status: "ok",
      data: { session_id: "stale-session", connection_id: tab.connectionId },
    });
    await firstConnect;

    expect(tab.sessionId).toBe("latest-session");
    expect(tab.state).toBe("connected");
    expect(apiMocks.sessionDisconnect).toHaveBeenCalledWith("stale-session");
  });

  it("ignores a stale error after the latest connection succeeds", async () => {
    const first = deferred<{
      status: "error";
      error: { code: string; message: string };
    }>();
    const second = deferred<{
      status: "ok";
      data: { session_id: string; connection_id: string };
    }>();
    apiMocks.sessionConnect
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const tab = createTab();
    tabs.tabs = [tab];

    const firstConnect = tabs.connect(tab.id);
    const secondConnect = tabs.connect(tab.id);
    second.resolve({
      status: "ok",
      data: { session_id: "latest-session", connection_id: tab.connectionId },
    });
    await secondConnect;
    first.resolve({
      status: "error",
      error: { code: "connect_failed", message: "stale failure" },
    });
    await firstConnect;

    expect(tab.sessionId).toBe("latest-session");
    expect(tab.state).toBe("connected");
    expect(tab.error).toBeNull();
  });

  it("does not show a host-key prompt after the tab closes", async () => {
    const pending = deferred<{
      status: "error";
      error: {
        code: string;
        message: string;
        host_key: {
          runtime_context_id: string;
          host: string;
          port: number;
          algorithm: string;
          fingerprint: string;
          key_line: string;
          changed: boolean;
        };
      };
    }>();
    apiMocks.sessionConnect.mockReturnValue(pending.promise);
    const tab = createTab();
    tabs.tabs = [tab];
    const connecting = tabs.connect(tab.id);
    tabs.close(tab.id);
    pending.resolve({
      status: "error",
      error: {
        code: "host_key_prompt",
        message: "Verify host key",
        host_key: {
          host: "example.com",
          port: 22,
          algorithm: "ssh-ed25519",
          fingerprint: "SHA256:test",
          key_line: "ssh-ed25519 test",
          changed: false,
          runtime_context_id: "context-a",
        },
      },
    });
    await connecting;

    expect(hostKeyMocks.ask).not.toHaveBeenCalled();
  });

  it("does not restore an old host-key prompt after access is revoked and unlocked again", async () => {
    const pending = deferred<{
      status: "error";
      error: {
        code: string;
        message: string;
        host_key: {
          runtime_context_id: string;
          vault_access_epoch: string;
          host: string;
          port: number;
          algorithm: string;
          fingerprint: string;
          key_line: string;
          changed: boolean;
        };
      };
    }>();
    apiMocks.sessionConnect.mockReturnValue(pending.promise);
    const tab = createTab();
    tabs.tabs = [tab];

    const connecting = tabs.connect(tab.id);
    vaultMocks.data = null;
    vaultMocks.accessGeneration += 1;
    vaultMocks.data = {};
    pending.resolve({
      status: "error",
      error: {
        code: "host_key_prompt",
        message: "Verify host key",
        host_key: {
          runtime_context_id: "old-context",
          vault_access_epoch: "old-access",
          host: "example.com",
          port: 22,
          algorithm: "ssh-ed25519",
          fingerprint: "SHA256:test",
          key_line: "ssh-ed25519 test",
          changed: false,
        },
      },
    });
    await connecting;
    expect(hostKeyMocks.ask).not.toHaveBeenCalled();
  });
});
