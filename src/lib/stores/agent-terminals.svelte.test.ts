// @vitest-environment jsdom

// The terminal registry is fed from effects in the terminal views: calls
// that change nothing must not write state, or those effects re-trigger
// themselves forever (Svelte's effect_update_depth_exceeded freezes the UI).

import { flushSync } from "svelte";
import { expect, it } from "vitest";
import { inertAgentApi, inertAgentEvents } from "../../test/agent-fakes";
import { AgentStore } from "./agent.svelte";

it("re-activating the active terminal from an effect settles", () => {
  const store = new AgentStore(inertAgentApi(), inertAgentEvents());
  store.terminalOpened("s1", "t1");
  store.terminalOpened("s1", "t2");
  let runs = 0;
  const stop = $effect.root(() => {
    $effect(() => {
      runs += 1;
      store.terminalActivated("s1", "t2");
    });
  });
  flushSync();
  expect(store.activeTerminal("s1")).toBe("t2");
  expect(runs).toBeLessThanOrEqual(2);
  stop();
});
