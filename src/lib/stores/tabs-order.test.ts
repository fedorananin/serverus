// Tab strip ordering (drag reorder / ⌘⇧←→). Connection lifecycle is covered
// in tabs.test.ts.

import { beforeEach, describe, expect, it, vi } from "vitest";
import { tabs, type Tab } from "./tabs.svelte";

vi.mock("$lib/api", () => ({
  commands: {},
  events: {
    sessionStateEvent: { listen: vi.fn(async () => () => {}) },
  },
  errorMessage: (error: unknown) => String(error),
  isApiError: () => false,
  unwrap: async (promise: Promise<unknown>) => promise,
}));

vi.mock("./hostkey.svelte", () => ({
  hostKey: { ask: vi.fn() },
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
    state: "connected",
    error: null,
    connectMessage: null,
    lastRemoteDir: null,
    reconnectAttempts: 0,
  };
}

describe("TabsStore tab order", () => {
  beforeEach(() => {
    tabs.tabs = [createTab("tab-a"), createTab("tab-b"), createTab("tab-c")];
    tabs.activeId = "tab-a";
  });

  it("moves a tab to a new index without touching the active tab", () => {
    tabs.move("tab-a", 2);

    expect(tabs.tabs.map((t) => t.id)).toEqual(["tab-b", "tab-c", "tab-a"]);
    expect(tabs.activeId).toBe("tab-a");
  });

  it("clamps the target index to the strip", () => {
    tabs.move("tab-c", -5);
    expect(tabs.tabs.map((t) => t.id)).toEqual(["tab-c", "tab-a", "tab-b"]);

    tabs.move("tab-c", 99);
    expect(tabs.tabs.map((t) => t.id)).toEqual(["tab-a", "tab-b", "tab-c"]);
  });

  it("ignores an unknown tab id", () => {
    tabs.move("tab-nope", 1);

    expect(tabs.tabs.map((t) => t.id)).toEqual(["tab-a", "tab-b", "tab-c"]);
  });
});
