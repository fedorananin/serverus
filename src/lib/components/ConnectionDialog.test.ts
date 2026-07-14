// @vitest-environment jsdom

import { fireEvent, render, screen, waitFor } from "@testing-library/svelte";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ConnectionSecrets, PublicConnection } from "$lib/api";
import ConnectionDialog from "./ConnectionDialog.svelte";

const apiMocks = vi.hoisted(() => ({
  connectionSecrets: vi.fn(),
}));

const vaultMocks = vi.hoisted(() => ({
  upsertConnection: vi.fn(),
}));

vi.mock("$lib/api", () => ({
  commands: {
    connectionSecrets: apiMocks.connectionSecrets,
    localHome: vi.fn(async () => ({ status: "ok", data: "/Users/test" })),
    sshKeyReadFile: vi.fn(async () => ({ status: "ok", data: "key" })),
  },
  errorMessage: (error: unknown) =>
    error instanceof Error
      ? error.message
      : typeof error === "object" && error !== null && "message" in error
        ? String(error.message)
        : String(error),
  unwrap: async (promise: Promise<{ status: "ok"; data: unknown } | { status: "error"; error: unknown }>) => {
    const result = await promise;
    if (result.status === "error") throw result.error;
    return result.data;
  },
}));

vi.mock("$lib/stores/vault.svelte", () => ({
  vault: {
    data: { connections: {} },
    requireRuntimeEpoch: () => 2,
    upsertConnection: vaultMocks.upsertConnection,
  },
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
}));

const existing: PublicConnection = {
  id: "connection-a",
  name: "Production",
  badge: null,
  protocol: "ssh",
  host: "prod.example.com",
  port: 22,
  auth: {
    method: "password",
    username: "deploy",
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

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function secrets(password: string): { status: "ok"; data: ConnectionSecrets } {
  return {
    status: "ok",
    data: { password, key_passphrase: null, key_inline: null },
  };
}

describe("ConnectionDialog credential loading", () => {
  beforeEach(() => {
    apiMocks.connectionSecrets.mockReset();
    vaultMocks.upsertConnection.mockReset();
    vaultMocks.upsertConnection.mockResolvedValue(undefined);
  });

  it("blocks Save and form submission until stored credentials are loaded", async () => {
    const pending = deferred<{ status: "ok"; data: ConnectionSecrets }>();
    apiMocks.connectionSecrets.mockReturnValue(pending.promise);
    const onclose = vi.fn();
    const { container } = render(ConnectionDialog, {
      existing,
      parentFolder: null,
      onclose,
    });
    const save = screen.getByRole("button", { name: "Save" });

    expect(save).toBeDisabled();
    await fireEvent.submit(container.querySelector("form")!);
    expect(vaultMocks.upsertConnection).not.toHaveBeenCalled();

    pending.resolve(secrets("stored-password"));
    await waitFor(() => expect(save).toBeEnabled());
    await fireEvent.click(save);

    await waitFor(() => expect(vaultMocks.upsertConnection).toHaveBeenCalledOnce());
    expect(vaultMocks.upsertConnection.mock.calls[0]?.[1]).toMatchObject({
      password: "stored-password",
    });
    expect(onclose).toHaveBeenCalledOnce();
  });

  it("keeps saving blocked after a load failure and retries explicitly", async () => {
    const retry = deferred<{ status: "ok"; data: ConnectionSecrets }>();
    apiMocks.connectionSecrets
      .mockResolvedValueOnce({
        status: "error",
        error: { code: "vault_read_failed", message: "Vault read failed" },
      })
      .mockReturnValueOnce(retry.promise);
    render(ConnectionDialog, {
      existing,
      parentFolder: null,
      onclose: vi.fn(),
    });

    expect(await screen.findByText("Could not load saved credentials: Vault read failed")).toBeInTheDocument();
    const save = screen.getByRole("button", { name: "Save" });
    expect(save).toBeDisabled();

    await fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(save).toBeDisabled();
    retry.resolve(secrets("recovered-password"));
    await waitFor(() => expect(save).toBeEnabled());
  });

  it("ignores a credential response for an older dialog target", async () => {
    const first = deferred<{ status: "ok"; data: ConnectionSecrets }>();
    const second = deferred<{ status: "ok"; data: ConnectionSecrets }>();
    apiMocks.connectionSecrets
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    const onclose = vi.fn();
    const view = render(ConnectionDialog, {
      existing,
      parentFolder: null,
      onclose,
    });
    const save = screen.getByRole("button", { name: "Save" });

    await view.rerender({
      existing: { ...existing, id: "connection-b", name: "Staging" },
      parentFolder: null,
      onclose,
    });
    first.resolve(secrets("obsolete-password"));
    await Promise.resolve();
    expect(save).toBeDisabled();

    second.resolve(secrets("current-password"));
    await waitFor(() => expect(save).toBeEnabled());
    await fireEvent.click(save);
    await waitFor(() => expect(vaultMocks.upsertConnection).toHaveBeenCalledOnce());
    expect(vaultMocks.upsertConnection).toHaveBeenCalledWith(
      "connection-b",
      expect.objectContaining({ password: "current-password" }),
      null,
    );
  });
});
