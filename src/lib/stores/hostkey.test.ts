import { beforeEach, describe, expect, it, vi } from "vitest";
import { hostKey } from "./hostkey.svelte";

const apiMocks = vi.hoisted(() => {
  type Result =
    | { status: "ok"; data: null }
    | { status: "error"; error: { code: string; message: string } };
  return {
    hostKeyAccept: vi.fn(async (): Promise<Result> => ({ status: "ok", data: null })),
  };
});

vi.mock("$lib/api", () => ({
  commands: {
    hostKeyAccept: apiMocks.hostKeyAccept,
  },
  isApiError: (error: unknown) =>
    typeof error === "object" && error !== null && "code" in error && "message" in error,
  unwrap: async (
    promise: Promise<
      | { status: "ok"; data: unknown }
      | { status: "error"; error: { code: string; message: string } }
    >,
  ) => {
    const result = await promise;
    if (result.status === "error") throw result.error;
    return result.data;
  },
}));

const prompt = {
  runtime_context_id: "context-a",
  vault_access_epoch: "access-a",
  host: "example.com",
  port: 22,
  algorithm: "ssh-ed25519",
  fingerprint: "SHA256:test",
  key_line: "ssh-ed25519 test",
  changed: false,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

describe("HostKeyStore runtime context", () => {
  beforeEach(() => {
    hostKey.pending = null;
    apiMocks.hostKeyAccept.mockClear();
  });

  it("clears a retired prompt without running either callback", () => {
    const accepted = vi.fn();
    const rejected = vi.fn();
    hostKey.ask(prompt, { accepted, rejected });

    hostKey.clearForContextRetirement();

    expect(hostKey.pending).toBeNull();
    expect(accepted).not.toHaveBeenCalled();
    expect(rejected).not.toHaveBeenCalled();
  });

  it("clears an access-revoked prompt without running either callback", () => {
    const accepted = vi.fn();
    const rejected = vi.fn();
    hostKey.ask(prompt, { accepted, rejected });

    hostKey.clearForAccessRevocation();

    expect(hostKey.pending).toBeNull();
    expect(accepted).not.toHaveBeenCalled();
    expect(rejected).not.toHaveBeenCalled();
  });

  it("accepts a host key only for the prompt runtime context", async () => {
    const accepted = vi.fn();
    hostKey.ask(prompt, { accepted, rejected: vi.fn() });

    await hostKey.accept();

    expect(apiMocks.hostKeyAccept).toHaveBeenCalledWith(
      prompt.host,
      prompt.port,
      prompt.key_line,
      prompt.runtime_context_id,
      prompt.vault_access_epoch,
    );
    expect(accepted).toHaveBeenCalledOnce();
  });

  it("does not reconnect when an in-flight acceptance finishes after retirement", async () => {
    const pending = deferred<{ status: "ok"; data: null }>();
    apiMocks.hostKeyAccept.mockReturnValueOnce(pending.promise);
    const accepted = vi.fn();
    const rejected = vi.fn();
    hostKey.ask(prompt, { accepted, rejected });

    const accepting = hostKey.accept();
    hostKey.clearForContextRetirement();
    pending.resolve({ status: "ok", data: null });
    await accepting;

    expect(accepted).not.toHaveBeenCalled();
    expect(rejected).not.toHaveBeenCalled();
  });

  it("suppresses a stale acceptance failure after access revocation", async () => {
    const pending = deferred<{ status: "ok"; data: null }>();
    apiMocks.hostKeyAccept.mockReturnValueOnce(pending.promise);
    const accepted = vi.fn();
    const rejected = vi.fn();
    hostKey.ask(prompt, { accepted, rejected });

    const accepting = hostKey.accept();
    hostKey.clearForAccessRevocation();
    pending.reject({ code: "wrong_runtime_context", message: "Vault access retired" });

    await expect(accepting).resolves.toBeUndefined();
    expect(accepted).not.toHaveBeenCalled();
    expect(rejected).not.toHaveBeenCalled();
  });

  it("suppresses a stale authorization rejection that arrives before the lock event", async () => {
    const error = { code: "wrong_runtime_context", message: "Vault access retired" };
    apiMocks.hostKeyAccept.mockResolvedValueOnce({ status: "error", error });
    const accepted = vi.fn();
    const rejected = vi.fn();
    hostKey.ask(prompt, { accepted, rejected });

    await expect(hostKey.accept()).resolves.toBeUndefined();

    expect(hostKey.pending).toBeNull();
    expect(accepted).not.toHaveBeenCalled();
    expect(rejected).not.toHaveBeenCalled();
  });

  it("keeps non-lifecycle acceptance failures observable", async () => {
    const error = { code: "io", message: "Could not save the key" };
    apiMocks.hostKeyAccept.mockResolvedValueOnce({ status: "error", error });
    const accepted = vi.fn();
    const rejected = vi.fn();
    hostKey.ask(prompt, { accepted, rejected });

    await expect(hostKey.accept()).rejects.toEqual(error);

    expect(accepted).not.toHaveBeenCalled();
    expect(rejected).not.toHaveBeenCalled();
  });
});
