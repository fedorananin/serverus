import { beforeEach, describe, expect, it, vi } from "vitest";
import type { PublicVault } from "$lib/api";
import { vault } from "./vault.svelte";

let vaultLockedHandler: (() => void) | null = null;

const apiMocks = vi.hoisted(() => ({
  vaultGetInfo: vi.fn(),
  vaultLock: vi.fn(),
  vaultSwitchPath: vi.fn(),
  treeUpdate: vi.fn(),
}));

vi.mock("$lib/api", () => ({
  commands: apiMocks,
  events: {
    vaultLockedEvent: {
      listen: vi.fn(async (handler: () => void) => {
        vaultLockedHandler = handler;
        return () => {};
      }),
    },
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

const vaultInfo = {
  path: "/vaults/next.serverus",
  exists: true,
  unlocked: false,
  biometry_available: true,
  quick_unlock_ready: false,
  quick_unlock_method: "Touch ID",
};

const oldVault: PublicVault = {
  tree: [],
  connections: {},
  known_hosts: {},
  settings: {
    security: { auto_lock_minutes: 5, lock_on_sleep: true, touch_id: false },
    transfers: {
      max_parallel_per_server: 2,
      conflict_policy: "ask",
      preserve_mtime: true,
      tar_acceleration: true,
    },
    editor: { use_system_default: true, custom_app: null },
    terminal: {
      font_family: "monospace",
      font_size: 13,
      scrollback: 1_000,
      copy_on_select: false,
    },
    panels: { show_hidden: false, size_format: "kib", default_local_dir: null },
  },
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

describe("VaultStore context retirement", () => {
  beforeEach(() => {
    vault.data = null;
    vault.busy = false;
    vault.error = null;
    vault.onContextRetired(() => {});
    vault.onAccessRevoked(() => {});
    vaultLockedHandler = null;
    apiMocks.vaultGetInfo.mockReset();
    apiMocks.vaultLock.mockReset();
    apiMocks.vaultSwitchPath.mockReset();
    apiMocks.treeUpdate.mockReset();
    apiMocks.vaultGetInfo.mockResolvedValue({ status: "ok", data: vaultInfo });
    apiMocks.vaultLock.mockResolvedValue({ status: "ok", data: null });
  });

  it("retires frontend context after the backend switch succeeds and before refresh", async () => {
    const operations: string[] = [];
    vault.data = oldVault;
    apiMocks.vaultSwitchPath.mockImplementation(async () => {
      operations.push("switch");
      return { status: "ok", data: null };
    });
    apiMocks.vaultGetInfo.mockImplementation(async () => {
      operations.push("refresh");
      return { status: "ok", data: vaultInfo };
    });
    vault.onContextRetired(() => operations.push(`retire:${vault.data === null}`));

    await vault.switchVault(vaultInfo.path);

    expect(operations).toEqual(["switch", "retire:true", "refresh"]);
  });

  it("does not retire frontend context when switching fails before commit", async () => {
    const retire = vi.fn();
    vault.onContextRetired(retire);
    apiMocks.vaultSwitchPath.mockResolvedValue({
      status: "error",
      error: { code: "vault_switch_failed", message: "Switch failed" },
    });

    await vault.switchVault(vaultInfo.path);

    expect(retire).not.toHaveBeenCalled();
    expect(apiMocks.vaultGetInfo).not.toHaveBeenCalled();
    expect(vault.error).toBe("Switch failed");
  });

  it("retires and refreshes after a committed switch reports cleanup failure", async () => {
    const operations: string[] = [];
    vault.onContextRetired(() => operations.push("retire"));
    apiMocks.vaultSwitchPath.mockImplementation(async () => {
      operations.push("switch");
      return {
        status: "error",
        error: {
          code: "runtime_cleanup_failed",
          message: "Old context cleanup failed",
        },
      };
    });
    apiMocks.vaultGetInfo.mockImplementation(async () => {
      operations.push("refresh");
      return { status: "ok", data: vaultInfo };
    });

    await vault.switchVault(vaultInfo.path);

    expect(operations).toEqual(["switch", "retire", "refresh"]);
    expect(vault.error).toBe("Old context cleanup failed");
  });

  it("keeps frontend context alive for an ordinary vault lock", async () => {
    const retire = vi.fn();
    const revokeAccess = vi.fn();
    vault.onContextRetired(retire);
    vault.onAccessRevoked(revokeAccess);

    await vault.lock();

    expect(retire).not.toHaveBeenCalled();
    expect(revokeAccess).toHaveBeenCalledOnce();
  });

  it("revokes frontend access when the backend emits VaultLocked", async () => {
    const revokeAccess = vi.fn();
    vault.data = oldVault;
    vault.onAccessRevoked(revokeAccess);
    await vault.init();

    expect(vaultLockedHandler).toBeTypeOf("function");
    (vaultLockedHandler as unknown as () => void)();

    expect(vault.data).toBeNull();
    expect(revokeAccess).toHaveBeenCalledOnce();
  });

  it("revokes frontend access after a committed vault switch", async () => {
    const revokeAccess = vi.fn();
    vault.onAccessRevoked(revokeAccess);
    apiMocks.vaultSwitchPath.mockResolvedValue({ status: "ok", data: null });

    await vault.switchVault(vaultInfo.path);

    expect(revokeAccess).toHaveBeenCalledOnce();
  });

  it("ignores a mutation response that arrives after the vault locks", async () => {
    const pending = deferred<{ status: "ok"; data: PublicVault }>();
    vault.data = oldVault;
    apiMocks.treeUpdate.mockReturnValueOnce(pending.promise);

    const mutation = vault.updateTree([]);
    await vault.lock();
    pending.resolve({ status: "ok", data: oldVault });
    await mutation;

    expect(vault.data).toBeNull();
  });

  it("ignores a concurrent switch while one is in flight", async () => {
    const pending = deferred<{ status: "ok"; data: null }>();
    apiMocks.vaultSwitchPath.mockReturnValueOnce(pending.promise);

    const first = vault.switchVault("/vaults/first.serverus");
    expect(vault.busy).toBe(true);
    const second = vault.switchVault("/vaults/second.serverus");

    expect(apiMocks.vaultSwitchPath).toHaveBeenCalledOnce();
    pending.resolve({ status: "ok", data: null });
    await Promise.all([first, second]);
    expect(vault.busy).toBe(false);
  });

  it("still refreshes and releases busy when a retirement listener throws", async () => {
    vault.data = oldVault;
    apiMocks.vaultSwitchPath.mockResolvedValue({ status: "ok", data: null });
    vault.onContextRetired(() => {
      throw new Error("retirement listener failed");
    });

    await expect(vault.switchVault(vaultInfo.path)).rejects.toThrow("retirement listener failed");

    expect(vault.data).toBeNull();
    expect(apiMocks.vaultGetInfo).toHaveBeenCalledOnce();
    expect(vault.busy).toBe(false);
  });
});
