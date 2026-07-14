import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { vault } from "$lib/stores/vault.svelte";
import * as terminalRouter from "$lib/terminals";
import { registerTerminal, unregisterTerminal } from "$lib/terminals";

const apiMocks = vi.hoisted(() => {
  let dataListener: ((event: { payload: Record<string, unknown> }) => void) | null = null;
  let exitListener: ((event: { payload: Record<string, unknown> }) => void) | null = null;
  return {
    listenData: vi.fn(async (listener: typeof dataListener) => {
      dataListener = listener;
      return () => {
        dataListener = null;
      };
    }),
    listenExit: vi.fn(async (listener: typeof exitListener) => {
      exitListener = listener;
      return () => {
        exitListener = null;
      };
    }),
    dataListener: () => dataListener,
    exitListener: () => exitListener,
  };
});

vi.mock("$lib/api", () => ({
  commands: {},
  events: {
    terminalDataEvent: { listen: apiMocks.listenData },
    terminalExitEvent: { listen: apiMocks.listenExit },
  },
  errorMessage: (error: unknown) => String(error),
  isApiError: () => false,
  unwrap: async (promise: Promise<unknown>) => promise,
}));

describe("terminal event routing", () => {
  beforeEach(() => {
    (vault as typeof vault & { runtimeEpoch: number | null }).runtimeEpoch = 2;
  });

  afterEach(() => {
    unregisterTerminal("terminal-b");
    (vault as typeof vault & { runtimeEpoch: number | null }).runtimeEpoch = null;
  });

  it("ignores terminal data and exit events from the previous vault epoch", async () => {
    const sink = vi.fn();
    const exited = vi.fn();
    registerTerminal("terminal-b", sink, exited);
    await vi.waitFor(() => {
      expect(apiMocks.dataListener()).toBeTypeOf("function");
      expect(apiMocks.exitListener()).toBeTypeOf("function");
    });
    const dataListener = apiMocks.dataListener();
    const exitListener = apiMocks.exitListener();
    if (!dataListener || !exitListener) throw new Error("terminal listeners were not registered");

    dataListener({ payload: { context_epoch: 0, term_id: "terminal-b", data: "QQ==" } });
    exitListener({ payload: { context_epoch: 0, term_id: "terminal-b" } });
    expect(sink).not.toHaveBeenCalled();
    expect(exited).not.toHaveBeenCalled();

    dataListener({ payload: { context_epoch: 2, term_id: "terminal-b", data: "QQ==" } });
    exitListener({ payload: { context_epoch: 2, term_id: "terminal-b" } });
    expect(sink).toHaveBeenCalledWith(new Uint8Array([65]));
    expect(exited).toHaveBeenCalledOnce();
  });

  it("drops registered terminal sinks at a vault boundary", async () => {
    const sink = vi.fn();
    const exited = vi.fn();
    registerTerminal("terminal-b", sink, exited);
    await vi.waitFor(() => expect(apiMocks.dataListener()).toBeTypeOf("function"));

    (
      terminalRouter as typeof terminalRouter & {
        resetTerminalContext?: () => void;
      }
    ).resetTerminalContext?.();
    apiMocks.dataListener()?.({
      payload: { context_epoch: 2, term_id: "terminal-b", data: "QQ==" },
    });
    apiMocks.exitListener()?.({
      payload: { context_epoch: 2, term_id: "terminal-b" },
    });

    expect(sink).not.toHaveBeenCalled();
    expect(exited).not.toHaveBeenCalled();
  });
});
