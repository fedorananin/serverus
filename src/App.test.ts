// @vitest-environment jsdom

import { fireEvent, render, screen, waitFor, within } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PublicConnection, PublicVault, VaultInfo } from "$lib/api";
import App from "./App.svelte";
import { dnd } from "$lib/stores/dnd.svelte";
import { hostKey } from "$lib/stores/hostkey.svelte";
import { tabs, type Tab } from "$lib/stores/tabs.svelte";
import { transfers } from "$lib/stores/transfers.svelte";
import { vault } from "$lib/stores/vault.svelte";

const apiMocks = vi.hoisted(() => {
  const state = {
    switchError: null as null | "preflight" | "closed",
    info: {
      context_epoch: 0,
      path: "/vault-a.serverus",
      exists: true,
      unlocked: true,
      biometry_available: true,
      quick_unlock_ready: false,
      quick_unlock_method: "Touch ID",
    },
  };
  const ok = (data: unknown) => Promise.resolve({ status: "ok" as const, data });
  const commandStubs = new Map<PropertyKey, ReturnType<typeof vi.fn>>();
  const commands = new Proxy(
    {
      connectionSecrets: vi.fn(() =>
        ok({ password: "vault-a-secret", key_passphrase: null, key_inline: null }),
      ),
      sessionConnect: vi.fn(() =>
        ok({ session_id: "session-a", connection_id: "connection-a" }),
      ),
      vaultGetInfo: vi.fn(() => ok(state.info)),
      vaultSwitchPath: vi.fn((path: string, contextEpoch: number) => {
        if (state.switchError === "preflight") {
          return Promise.resolve({
            status: "error" as const,
            error: { code: "io", message: "Could not save the vault path" },
          });
        }
        if (state.switchError === "closed") {
          state.info = { ...state.info, context_epoch: contextEpoch + 1 };
          return Promise.resolve({
            status: "error" as const,
            error: { code: "vault_context_closed", message: "Teardown failed" },
          });
        }
        state.info = {
          ...state.info,
          context_epoch: contextEpoch + 2,
          path,
          unlocked: false,
        };
        return ok(null);
      }),
    },
    {
      get(target, property, receiver) {
        if (Reflect.has(target, property)) return Reflect.get(target, property, receiver);
        let stub = commandStubs.get(property);
        if (!stub) {
          stub = vi.fn(() => ok(null));
          commandStubs.set(property, stub);
        }
        return stub;
      },
    },
  );
  type EventListener = (event: { payload: Record<string, unknown> }) => void;
  const eventListeners = new Map<PropertyKey, Set<EventListener>>();
  const events = new Proxy(
    {},
    {
      get(_target, property) {
        return {
          listen: vi.fn(async (listener: EventListener) => {
            let listeners = eventListeners.get(property);
            if (!listeners) {
              listeners = new Set();
              eventListeners.set(property, listeners);
            }
            listeners.add(listener);
            return () => listeners?.delete(listener);
          }),
        };
      },
    },
  );
  const listener = (property: PropertyKey) =>
    Array.from(eventListeners.get(property) ?? [])[0];
  return { commands, events, listener, state };
});

vi.mock("$lib/api", () => ({
  commands: apiMocks.commands,
  events: apiMocks.events,
  errorMessage: (error: unknown) =>
    typeof error === "object" && error !== null && "message" in error
      ? String(error.message)
      : String(error),
  isApiError: (error: unknown) =>
    typeof error === "object" && error !== null && "code" in error && "message" in error,
  unwrap: async (
    promise: Promise<
      { status: "ok"; data: unknown } | { status: "error"; error: unknown }
    >,
  ) => {
    const result = await promise;
    if (result.status === "error") throw result.error;
    return result.data;
  },
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => null),
}));

const connection: PublicConnection = {
  id: "connection-a",
  name: "Vault A server",
  badge: null,
  protocol: "ssh",
  host: "a.example.com",
  port: 22,
  auth: {
    method: "password",
    username: "alice",
    key_path: null,
    has_password: true,
    has_key_inline: false,
    has_key_passphrase: false,
  },
  jump_host: null,
  ftp: null,
  s3: null,
  remote_dir: null,
  local_dir: null,
  tunnels: [],
  disable_terminal: false,
  notes: "",
};

function vaultData(): PublicVault {
  return {
    tree: [{ type: "connection", id: connection.id }],
    connections: { [connection.id]: connection },
    known_hosts: {},
    settings: {
      security: { auto_lock_minutes: 15, lock_on_sleep: true, touch_id: true },
      transfers: {
        max_parallel_per_server: 5,
        conflict_policy: "ask",
        preserve_mtime: true,
        tar_acceleration: true,
      },
      editor: { use_system_default: true, custom_app: null },
      terminal: {
        font_family: "monospace",
        font_size: 13,
        scrollback: 10_000,
        copy_on_select: false,
      },
      panels: { show_hidden: false, size_format: "kib", default_local_dir: null },
    },
  };
}

function liveTab(): Tab {
  return {
    id: "tab-a",
    connectionId: connection.id,
    view: "terminal",
    sessionId: null,
    state: "connecting",
    error: null,
    connectMessage: null,
    lastRemoteDir: null,
    reconnectAttempts: 0,
  };
}

describe("vault UI isolation", () => {
  beforeEach(() => {
    apiMocks.state.info = {
      context_epoch: 0,
      path: "/vault-a.serverus",
      exists: true,
      unlocked: true,
      biometry_available: true,
      quick_unlock_ready: false,
      quick_unlock_method: "Touch ID",
    };
    apiMocks.state.switchError = null;
    vault.info = { ...apiMocks.state.info } as VaultInfo;
    vault.runtimeEpoch = 0;
    vault.data = vaultData();
    tabs.tabs = [liveTab()];
    tabs.activeId = null;
  });

  afterEach(() => {
    vault.data = null;
    vault.info = null;
    tabs.tabs = [];
    tabs.activeId = null;
    transfers.items = [];
    hostKey.pending = null;
    dnd.active = null;
    dnd.label = "";
  });

  it("keeps session UI mounted on lock but destroys a secret-bearing dialog", async () => {
    const { container } = render(App);

    await fireEvent.contextMenu(screen.getByRole("treeitem", { name: /Vault A server/ }));
    await fireEvent.click(screen.getByRole("menuitem", { name: "Edit…" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit Vault A server" });
    const password = within(dialog).getByLabelText(/Password/);
    await fireEvent.input(password, { target: { value: "vault-a-secret" } });
    expect(password).toHaveValue("vault-a-secret");

    vault.data = null;

    await waitFor(() => {
      expect(container.querySelector(".main")).toBeInTheDocument();
      expect(
        screen.queryByRole("dialog", { name: "Edit Vault A server" }),
      ).not.toBeInTheDocument();
    });
    expect(tabs.tabs).toHaveLength(1);
  });

  it("clears the complete frontend context after a successful switch", async () => {
    const { container } = render(App);
    vault.data = null;
    await waitFor(() => expect(container.querySelector(".main")).toBeInTheDocument());
    const masterPassword = screen.getByPlaceholderText("Master password");
    await fireEvent.input(masterPassword, { target: { value: "vault-a-master-password" } });
    transfers.items = [{ id: "transfer-a" } as (typeof transfers.items)[number]];
    transfers.summary = {
      queued: 1,
      running: 0,
      done: 0,
      failed: 0,
      total_items: 1,
    };
    hostKey.pending = {
      host: "a.example.com",
      port: 22,
      algorithm: "ssh-ed25519",
      fingerprint: "SHA256:test",
      key_line: "ssh-ed25519 test",
      changed: false,
      accepted: vi.fn(),
      rejected: vi.fn(),
    };
    dnd.active = { kind: "files", side: "remote" };
    dnd.label = "secret.txt";

    await vault.switchVault("/vault-b.serverus");

    expect(tabs.tabs).toHaveLength(0);
    expect(transfers.items).toHaveLength(0);
    expect(transfers.summary.total_items).toBe(0);
    expect(hostKey.pending).toBeNull();
    expect(dnd.active).toBeNull();
    expect(dnd.label).toBe("");
    await waitFor(() => expect(container.querySelector(".main")).not.toBeInTheDocument());
    expect(screen.getByPlaceholderText("Master password")).toHaveValue("");
  });

  it("preserves vault A UI when switching fails before teardown", async () => {
    render(App);
    vault.data = null;
    const password = await screen.findByPlaceholderText("Master password");
    await fireEvent.input(password, { target: { value: "vault-a-master-password" } });
    apiMocks.state.switchError = "preflight";

    await vault.switchVault("/vault-b.serverus");

    expect(tabs.tabs).toHaveLength(1);
    expect(password).toHaveValue("vault-a-master-password");
  });

  it("fails closed after teardown and retries with vault A's even epoch", async () => {
    render(App);
    vault.data = null;
    await screen.findByPlaceholderText("Master password");
    apiMocks.state.switchError = "closed";

    await vault.switchVault("/vault-b.serverus");

    expect(tabs.tabs).toHaveLength(0);
    expect(vault.data).toBeNull();

    apiMocks.state.switchError = null;
    await vault.switchVault("/vault-b.serverus");

    expect(apiMocks.commands.vaultSwitchPath).toHaveBeenLastCalledWith(
      "/vault-b.serverus",
      0,
    );
    expect(apiMocks.state.info.context_epoch).toBe(2);
  });

  it("clears old toasts and ignores late edit events from vault A", async () => {
    render(App);
    await vi.waitFor(() =>
      expect(apiMocks.listener("remoteEditUploadedEvent")).toBeTypeOf("function"),
    );
    const listener = apiMocks.listener("remoteEditUploadedEvent");
    if (!listener) throw new Error("remote-edit listener was not registered");
    listener({
      payload: {
        context_epoch: 0,
        name: "vault-a-secret.txt",
        remote_path: "/vault-a-secret.txt",
        error: null,
      },
    });
    expect(await screen.findByText("Uploaded vault-a-secret.txt ✓")).toBeInTheDocument();

    vault.data = null;
    await vault.switchVault("/vault-b.serverus");
    expect(screen.queryByText("Uploaded vault-a-secret.txt ✓")).not.toBeInTheDocument();

    listener({
      payload: {
        context_epoch: 0,
        name: "late-vault-a-secret.txt",
        remote_path: "/late-vault-a-secret.txt",
        error: null,
      },
    });
    expect(screen.queryByText(/late-vault-a-secret/)).not.toBeInTheDocument();
  });
});
