import { expect, it, vi } from "vitest";
import type { AppApi, TransferListDto } from "$lib/app/contracts/api";
import type { AppEventSource } from "$lib/app/contracts/events";
import { TransfersStore } from "./transfers.svelte";

const initialSnapshot: TransferListDto = {
  runtime_context_id: "context-current",
  items: [],
  summary: { queued: 0, running: 0, done: 0, failed: 0, total_items: 0 },
};

function dependencies() {
  const transfers = {
    list: vi.fn(async () => initialSnapshot),
    upload: vi.fn(async () => {}),
    download: vi.fn(async () => {}),
    pause: vi.fn(async () => {}),
    retry: vi.fn(async () => {}),
    resume: vi.fn(async () => {}),
    cancel: vi.fn(async () => {}),
    pauseAll: vi.fn(async () => {}),
    resumeAll: vi.fn(async () => {}),
    cancelAll: vi.fn(async () => {}),
    clearFinished: vi.fn(async () => {}),
    resolve: vi.fn(async () => {}),
  } satisfies AppApi["transfers"];
  const api: AppApi = {
    transfers,
    vault: { touchActivity: vi.fn(async () => {}) },
  };
  const events: AppEventSource = {
    transfers: { listenProgress: vi.fn(async () => () => {}) },
    remoteEdit: { listenUploaded: vi.fn(async () => () => {}) },
  };
  return { api, events, transfers };
}

it("delegates transfer commands to AppApi", async () => {
  const { api, events, transfers } = dependencies();
  const store = new TransfersStore(api, events);
  await store.init();

  await store.upload("session-a", "/local/a.txt", "/remote");
  await store.download("session-a", "/remote/b.txt", "/local");
  await store.pause("transfer-1");
  await store.retry("transfer-2");
  await store.resume("transfer-3");
  await store.cancel("transfer-4");
  await store.pauseAll();
  await store.resumeAll();
  await store.cancelAll();
  await store.clearFinished();
  await store.resolve("session-1", "transfer-5", "rename", true);

  expect(transfers.upload).toHaveBeenCalledWith("session-a", "/local/a.txt", "/remote");
  expect(transfers.download).toHaveBeenCalledWith("session-a", "/remote/b.txt", "/local");
  expect(transfers.pause).toHaveBeenCalledWith("transfer-1");
  expect(transfers.retry).toHaveBeenCalledWith("transfer-2");
  expect(transfers.resume).toHaveBeenCalledWith("transfer-3");
  expect(transfers.cancel).toHaveBeenCalledWith("transfer-4");
  expect(transfers.pauseAll).toHaveBeenCalledWith("context-current");
  expect(transfers.resumeAll).toHaveBeenCalledWith("context-current");
  expect(transfers.cancelAll).toHaveBeenCalledWith("context-current");
  expect(transfers.clearFinished).toHaveBeenCalledWith("context-current");
  expect(transfers.resolve).toHaveBeenCalledWith("session-1", "transfer-5", "rename", true);
});
