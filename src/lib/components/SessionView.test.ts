// @vitest-environment jsdom

import { render, screen } from "@testing-library/svelte";
import { flushSync } from "svelte";
import { describe, expect, it, vi } from "vitest";

import type { Tab } from "$lib/stores/tabs.svelte";
import SessionView from "./SessionView.svelte";

const vaultMock = vi.hoisted(() => ({
  data: null as null | { connections: Record<string, unknown> },
}));

vi.mock("$lib/stores/vault.svelte", () => ({ vault: vaultMock }));
vi.mock("$lib/stores/tabs.svelte", () => ({ tabs: { activeId: null } }));
// Only the terminal renders the stub, so its presence means "mounted".
vi.mock("./TerminalPanel.svelte", async () => import("../../test/Stub.svelte"));
vi.mock("./FilesView.svelte", async () => ({ default: () => {} }));
vi.mock("./TunnelsView.svelte", async () => ({ default: () => {} }));
vi.mock("./agent/AgentActivity.svelte", async () => ({ default: () => {} }));

function tab(): Tab {
  return {
    id: "tab-1",
    connectionId: "c1",
    view: "files",
    sessionId: "s1",
    state: "connected",
    error: null,
    connectMessage: null,
    lastRemoteDir: null,
    reconnectAttempts: 0,
  };
}

const connection = (overrides: object) => ({
  protocol: "ssh",
  disable_terminal: false,
  tunnels: [],
  auth: { username: "root" },
  host: "example.com",
  ...overrides,
});

describe("SessionView while the vault is locked", () => {
  it.each([
    ["an SFTP-only account", { disable_terminal: true }],
    ["an FTP session", { protocol: "ftp" }],
  ])("never mounts a terminal for %s", (_label, overrides) => {
    vaultMock.data = { connections: { c1: connection(overrides) } };
    const view = render(SessionView, { tab: tab() });
    expect(screen.queryByTestId("component-stub")).toBeNull();

    vaultMock.data = null;
    // The mock is not reactive; re-rendering re-reads it like a lock would.
    view.rerender({ tab: { ...tab() } });
    flushSync();
    expect(screen.queryByTestId("component-stub")).toBeNull();
  });

  it("keeps a shell account's terminal", () => {
    vaultMock.data = { connections: { c1: connection({}) } };
    render(SessionView, { tab: tab() });
    expect(screen.getByTestId("component-stub")).toBeTruthy();
  });
});
