import { beforeEach, describe, expect, it, vi } from "vitest";
import type { TransferProgressEvent } from "$lib/api";

type ProgressListener = (event: { payload: TransferProgressEvent }) => void;

const apiMocks = vi.hoisted(() => ({
  listener: null as ProgressListener | null,
  listen: vi.fn(async (listener: ProgressListener) => {
    apiMocks.listener = listener;
    return () => {};
  }),
  transferList: vi.fn(),
}));

vi.mock("$lib/api", () => ({
  commands: {
    transferList: apiMocks.transferList,
  },
  events: {
    transferProgressEvent: { listen: apiMocks.listen },
  },
  unwrap: async (promise: Promise<{ status: "ok"; data: unknown }>) =>
    (await promise).data,
}));

import { transfers } from "./transfers.svelte";

const emptySummary = {
  queued: 0,
  running: 0,
  done: 0,
  failed: 0,
  total_items: 0,
};

describe("TransfersStore vault event isolation", () => {
  beforeEach(async () => {
    apiMocks.transferList.mockReset();
    (transfers as typeof transfers & { resetVaultContext: () => void }).resetVaultContext?.();
    (transfers as typeof transfers & { setVaultContext: (epoch: number) => void })
      .setVaultContext?.(2);
    await transfers.init();
  });

  it("ignores a delayed progress snapshot from the previous vault epoch", () => {
    apiMocks.listener?.({
      payload: {
        context_epoch: 0,
        items: [{ id: "old-transfer" } as TransferProgressEvent["items"][number]],
        summary: { ...emptySummary, running: 1, total_items: 1 },
      },
    });

    expect(transfers.items).toHaveLength(0);
    expect(transfers.summary).toEqual(emptySummary);

    apiMocks.listener?.({
      payload: {
        context_epoch: 2,
        items: [{ id: "new-transfer" } as TransferProgressEvent["items"][number]],
        summary: { ...emptySummary, running: 1, total_items: 1 },
      },
    });

    expect(transfers.items[0]?.id).toBe("new-transfer");
  });

  it("ignores a delayed list response from the previous vault epoch", async () => {
    let resolve!: (value: {
      status: "ok";
      data: {
        context_epoch: number;
        items: TransferProgressEvent["items"];
        summary: typeof emptySummary;
      };
    }) => void;
    apiMocks.transferList.mockReturnValue(
      new Promise((resolvePromise) => {
        resolve = resolvePromise;
      }),
    );

    const refresh = transfers.refresh();
    (transfers as typeof transfers & { resetVaultContext: () => void }).resetVaultContext?.();
    (transfers as typeof transfers & { setVaultContext: (epoch: number) => void })
      .setVaultContext?.(4);
    resolve({
      status: "ok",
      data: {
        context_epoch: 2,
        items: [{ id: "old-transfer" } as TransferProgressEvent["items"][number]],
        summary: { ...emptySummary, running: 1, total_items: 1 },
      },
    });
    await refresh;

    expect(transfers.items).toHaveLength(0);
    expect(transfers.summary).toEqual(emptySummary);
  });
});
