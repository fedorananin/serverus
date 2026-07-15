import { describe, expect, it, vi } from "vitest";
import type {
  AppApi,
  TransferListDto,
  TransferSnapshot,
} from "$lib/app/contracts/api";
import type { AppEventSource } from "$lib/app/contracts/events";
import type { RemoteEditUploadedEvent } from "$lib/api";
import { TransfersStore } from "./transfers.svelte";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

function snapshot(items: TransferSnapshot[] = []): TransferListDto {
  return {
    runtime_context_id: "context-current",
    items,
    summary: {
      queued: items.filter((item) => item.state === "queued").length,
      running: items.filter((item) => item.state === "running").length,
      done: items.filter((item) => item.state === "done").length,
      failed: items.filter((item) => item.state === "error").length,
      total_items: items.length,
    },
  };
}

function transfer(id: string, state: TransferSnapshot["state"]): TransferSnapshot {
  return {
    id,
    session_id: `session-${id}`,
    kind: "upload",
    state,
    error: null,
    name: `${id}.txt`,
    local_path: `/local/${id}.txt`,
    remote_path: `/remote/${id}.txt`,
    accelerated: false,
    done: 0,
    total: 100,
    speed_bps: 0,
  };
}

class FakeAppApi implements AppApi {
  current: TransferListDto;

  readonly transfers;
  readonly vault = {
    touchActivity: vi.fn(async () => {}),
  };

  constructor(initial: TransferListDto = snapshot()) {
    this.current = initial;
    this.transfers = {
      list: vi.fn(async () => this.current),
      upload: vi.fn(async (_sessionId: string, _localPath: string, _remoteDir: string) => {}),
      download: vi.fn(async (_sessionId: string, _remotePath: string, _localDir: string) => {}),
      pause: vi.fn(async (_id: string) => {}),
      retry: vi.fn(async (_id: string) => {}),
      resume: vi.fn(async (_id: string) => {}),
      cancel: vi.fn(async (_id: string) => {}),
      pauseAll: vi.fn(async () => {}),
      resumeAll: vi.fn(async () => {}),
      cancelAll: vi.fn(async () => {}),
      clearFinished: vi.fn(async () => {}),
      resolve: vi.fn(
        async (
          _sessionId: string,
          _id: string,
          _action: "overwrite" | "skip" | "rename",
          _applyToAll: boolean,
        ) => {},
      ),
    };
  }
}

class FakeEventSource implements AppEventSource {
  private progressListener: ((value: TransferListDto) => void) | null = null;

  readonly transfers = {
    listenProgress: vi.fn(async (listener: (value: TransferListDto) => void) => {
      this.progressListener = listener;
      return () => {
        if (this.progressListener === listener) this.progressListener = null;
      };
    }),
  };

  readonly remoteEdit = {
    listenUploaded: vi.fn(
      async (_listener: (value: RemoteEditUploadedEvent) => void) => () => {},
    ),
  };

  emitProgress(value: TransferListDto) {
    this.progressListener?.(value);
  }
}

describe("TransfersStore", () => {
  it("creates independent stores", async () => {
    const firstEvents = new FakeEventSource();
    const secondEvents = new FakeEventSource();
    const first = new TransfersStore(
      new FakeAppApi(snapshot([transfer("first", "queued")])),
      firstEvents,
    );
    const second = new TransfersStore(
      new FakeAppApi(snapshot([transfer("second", "done")])),
      secondEvents,
    );

    await Promise.all([first.init(), second.init()]);
    firstEvents.emitProgress(snapshot([transfer("first-running", "running")]));

    expect(first).not.toBe(second);
    expect(first.items.map((item) => item.id)).toEqual(["first-running"]);
    expect(second.items.map((item) => item.id)).toEqual(["second"]);
  });

  it("loads the initial backend snapshot", async () => {
    const api = new FakeAppApi(snapshot([transfer("existing", "paused")]));
    const events = new FakeEventSource();
    const store = new TransfersStore(api, events);

    await store.init();

    expect(events.transfers.listenProgress).toHaveBeenCalledOnce();
    expect(api.transfers.list).toHaveBeenCalledOnce();
    expect(store.items.map((item) => item.id)).toEqual(["existing"]);
    expect(store.summary.total_items).toBe(1);
  });

  it("applies progress events and opens the queue for active work", async () => {
    const events = new FakeEventSource();
    const store = new TransfersStore(new FakeAppApi(), events);
    await store.init();

    events.emitProgress(snapshot([transfer("running", "running")]));

    expect(store.items.map((item) => item.id)).toEqual(["running"]);
    expect(store.summary.running).toBe(1);
    expect(store.collapsed).toBe(false);
  });

  it("preserves progress that arrives after the initial list request", async () => {
    const pendingList = deferred<TransferListDto>();
    const api = new FakeAppApi();
    api.transfers.list.mockReturnValueOnce(pendingList.promise);
    const events = new FakeEventSource();
    const store = new TransfersStore(api, events);
    const initialization = store.init();
    await vi.waitFor(() => expect(api.transfers.list).toHaveBeenCalledOnce());

    events.emitProgress(snapshot([transfer("newer-event", "running")]));
    pendingList.resolve(snapshot([transfer("older-list", "queued")]));
    await initialization;

    expect(store.items.map((item) => item.id)).toEqual(["newer-event"]);
    expect(store.summary.running).toBe(1);
    expect(store.collapsed).toBe(false);
  });

  it("clears the cached queue when its runtime context retires", async () => {
    const api = new FakeAppApi(
      snapshot([transfer("queued", "queued"), transfer("failed", "error")]),
    );
    const events = new FakeEventSource();
    const store = new TransfersStore(api, events);
    await store.init();

    store.retireContext();
    await store.init();

    expect(store.items).toEqual([]);
    expect(store.summary).toEqual({
      queued: 0,
      running: 0,
      done: 0,
      failed: 0,
      total_items: 0,
    });
    expect(store.active).toBe(false);
    expect(store.conflicted).toBeNull();
    expect(events.transfers.listenProgress).toHaveBeenCalledOnce();
    expect(api.transfers.list).toHaveBeenCalledTimes(2);
  });

  it("ignores a late progress event from the retired runtime context", async () => {
    const oldSnapshot = {
      ...snapshot([transfer("old", "running")]),
      runtime_context_id: "context-old",
    } as TransferListDto;
    const events = new FakeEventSource();
    const api = new FakeAppApi(oldSnapshot);
    const store = new TransfersStore(api, events);
    await store.init();

    store.retireContext();
    events.emitProgress({
      ...snapshot([transfer("zombie", "running")]),
      runtime_context_id: "context-old",
    } as TransferListDto);

    expect(store.items).toEqual([]);
    expect(store.summary.total_items).toBe(0);

    api.current = {
      ...snapshot(),
      runtime_context_id: "context-new",
    };
    await store.init();
    events.emitProgress({
      ...snapshot([transfer("new", "running")]),
      runtime_context_id: "context-new",
    });
    expect(store.items.map((item) => item.id)).toEqual(["new"]);
  });

  it("ignores an initial list response after its context retires", async () => {
    const pendingList = deferred<TransferListDto>();
    const api = new FakeAppApi();
    api.transfers.list.mockReturnValueOnce(pendingList.promise);
    const events = new FakeEventSource();
    const store = new TransfersStore(api, events);
    const initialization = store.init();
    await vi.waitFor(() => expect(api.transfers.list).toHaveBeenCalledOnce());

    store.retireContext();
    events.emitProgress({
      ...snapshot([transfer("stale-event-item", "running")]),
      runtime_context_id: "context-old",
    });
    expect(store.items).toEqual([]);
    pendingList.resolve({
      ...snapshot([transfer("stale-list-item", "running")]),
      runtime_context_id: "context-old",
    });
    await initialization;

    expect(store.items).toEqual([]);
    api.current = {
      ...snapshot([transfer("new-list-item", "queued")]),
      runtime_context_id: "context-new",
    };
    await store.init();
    expect(store.items.map((item) => item.id)).toEqual(["new-list-item"]);
    events.emitProgress({
      ...snapshot([transfer("new-context-item", "running")]),
      runtime_context_id: "context-new",
    });
    expect(store.items.map((item) => item.id)).toEqual(["new-context-item"]);
  });

});
