import { expect, it, vi } from "vitest";
import type { AppApi } from "./contracts/api";
import type { AppEventSource } from "./contracts/events";
import * as appModelModule from "./model.svelte";

const { createAppModel } = appModelModule;

function fakeApi(): AppApi {
  return {
    transfers: {
      list: vi.fn(async () => ({
        runtime_context_id: "context-a",
        items: [],
        summary: { queued: 0, running: 0, done: 0, failed: 0, total_items: 0 },
      })),
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
    },
    vault: {
      touchActivity: vi.fn(async () => {}),
    },
  };
}

function fakeEvents(): AppEventSource {
  return {
    transfers: {
      listenProgress: vi.fn(async () => () => {}),
    },
    remoteEdit: {
      listenUploaded: vi.fn(async () => () => {}),
    },
  };
}

it("creates an independent app-scoped model from injected ports", () => {
  const firstApi = fakeApi();
  const firstEvents = fakeEvents();
  const secondApi = fakeApi();
  const secondEvents = fakeEvents();

  const first = createAppModel(firstApi, firstEvents);
  const second = createAppModel(secondApi, secondEvents);

  expect(first.api).toBe(firstApi);
  expect(first.eventSource).toBe(firstEvents);
  expect(second.api).toBe(secondApi);
  expect(second.eventSource).toBe(secondEvents);
  expect(first.transfers).not.toBe(second.transfers);
});

it("wires vault retirement to tabs, host-key state, and transfers", () => {
  const wireContextRetirement = Reflect.get(appModelModule, "wireContextRetirement");
  expect(wireContextRetirement).toBeTypeOf("function");
  if (typeof wireContextRetirement !== "function") return;

  let retire: (() => void) | null = null;
  const stop = vi.fn();
  const vaultLifecycle = {
    onContextRetired(callback: () => void) {
      retire = callback;
      return stop;
    },
  };
  const tabsLifecycle = { retireContext: vi.fn() };
  const hostKeyLifecycle = { clearForContextRetirement: vi.fn() };
  const transfersLifecycle = { retireContext: vi.fn() };

  const unwire = wireContextRetirement(
    vaultLifecycle,
    tabsLifecycle,
    hostKeyLifecycle,
    transfersLifecycle,
  );
  expect(retire).toBeTypeOf("function");
  (retire as unknown as () => void)();

  expect(tabsLifecycle.retireContext).toHaveBeenCalledOnce();
  expect(hostKeyLifecycle.clearForContextRetirement).toHaveBeenCalledOnce();
  expect(transfersLifecycle.retireContext).toHaveBeenCalledOnce();
  expect(unwire).toBe(stop);
});

it("clears host-key state when vault access is revoked", () => {
  const wireAccessRevocation = Reflect.get(appModelModule, "wireAccessRevocation");
  expect(wireAccessRevocation).toBeTypeOf("function");
  if (typeof wireAccessRevocation !== "function") return;

  let revokeAccess: (() => void) | null = null;
  const stop = vi.fn();
  const vaultLifecycle = {
    onAccessRevoked(callback: () => void) {
      revokeAccess = callback;
      return stop;
    },
  };
  const hostKeyLifecycle = { clearForAccessRevocation: vi.fn() };

  const unwire = wireAccessRevocation(vaultLifecycle, hostKeyLifecycle);
  expect(revokeAccess).toBeTypeOf("function");
  (revokeAccess as unknown as () => void)();

  expect(hostKeyLifecycle.clearForAccessRevocation).toHaveBeenCalledOnce();
  expect(unwire).toBe(stop);
});
