// Tabs that hit host-key prompts at the same time, against the real prompt
// queue. Single-tab connection lifecycle is covered in tabs.test.ts.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { hostKey } from "./hostkey.svelte";
import { tabs, type Tab } from "./tabs.svelte";

const apiMocks = vi.hoisted(() => ({
  sessionConnect: vi.fn(),
  sessionDisconnect: vi.fn(async () => ({ status: "ok", data: null })),
  hostKeyAccept: vi.fn(async (_host: string) => ({ status: "ok", data: null })),
}));

vi.mock("$lib/api", () => ({
  commands: apiMocks,
  events: {
    sessionStateEvent: { listen: vi.fn(async () => () => {}) },
  },
  errorMessage: (error: unknown) => String(error),
  isApiError: (error: unknown) =>
    typeof error === "object" && error !== null && "code" in error && "message" in error,
  unwrap: async (promise: Promise<{ status: "ok"; data: unknown } | { status: "error"; error: unknown }>) => {
    const result = await promise;
    if (result.status === "error") throw result.error;
    return result.data;
  },
}));

vi.mock("./vault.svelte", () => ({
  vault: { accessGeneration: 1, data: {}, isAccessCurrent: () => true },
}));

function createTab(id: string): Tab {
  return {
    id,
    connectionId: `connection-${id}`,
    view: "terminal",
    sessionId: null,
    state: "connecting",
    error: null,
    connectMessage: null,
    lastRemoteDir: null,
    reconnectAttempts: 0,
  };
}

/** Each server asks about its key until the key is trusted. */
function serversAskingForTheirKeys() {
  const trusted = new Set<string>();
  apiMocks.hostKeyAccept.mockImplementation(async (host: string) => {
    trusted.add(host);
    return { status: "ok", data: null };
  });
  apiMocks.sessionConnect.mockImplementation(async (connectionId: string) => {
    const host = `${connectionId}.example.com`;
    if (trusted.has(host)) {
      return { status: "ok", data: { session_id: `session-${connectionId}` } };
    }
    return {
      status: "error",
      error: {
        code: "host_key_prompt",
        message: "Verify host key",
        host_key: {
          runtime_context_id: "context-a",
          vault_access_epoch: "access-a",
          host,
          port: 22,
          algorithm: "ssh-ed25519",
          fingerprint: `SHA256:${connectionId}`,
          key_line: `ssh-ed25519 ${connectionId}`,
          changed: false,
        },
      },
    };
  });
}

const tab = (id: string) => tabs.tabs.find((candidate) => candidate.id === id);

describe("TabsStore host-key prompts", () => {
  beforeEach(() => {
    tabs.tabs = [];
    tabs.activeId = null;
    hostKey.clearForContextRetirement();
    apiMocks.sessionConnect.mockReset();
    apiMocks.hostKeyAccept.mockReset();
    serversAskingForTheirKeys();
  });

  it("connects every tab whose prompt arrived while another was on screen", async () => {
    tabs.tabs = [createTab("a"), createTab("b"), createTab("c")];

    await Promise.all(["a", "b", "c"].map((id) => tabs.connect(id)));
    expect(hostKey.waiting).toBe(3);

    await hostKey.accept();
    hostKey.reject();
    await hostKey.accept();

    await vi.waitFor(() => {
      expect(tab("a")?.state).toBe("connected");
      expect(tab("c")?.state).toBe("connected");
    });
    expect(tab("b")?.state).toBe("error");
    expect(tab("b")?.error).toBe("Host key rejected");
    expect(hostKey.pending).toBeNull();
  });

  it("stops asking about a tab that was closed while its prompt waited", async () => {
    tabs.tabs = [createTab("a"), createTab("b")];

    await Promise.all(["a", "b"].map((id) => tabs.connect(id)));
    tabs.close("b");

    expect(hostKey.waiting).toBe(1);
    expect(hostKey.pending?.host).toBe("connection-a.example.com");
  });
});
