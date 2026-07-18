import { describe, expect, it } from "vitest";

import type { TransferSummary } from "$lib/app/contracts/api";
import { queueActivity, queueSettled } from "./transfer-settle";

function summary(partial: Partial<TransferSummary>): TransferSummary {
  return { queued: 0, running: 0, done: 0, failed: 0, total_items: 0, ...partial };
}

describe("queueSettled", () => {
  it("fires when the queue goes from busy to idle", () => {
    const busy = queueActivity(summary({ running: 2, done: 1, total_items: 3 }));
    const idle = queueActivity(summary({ done: 3, total_items: 3 }));
    expect(queueSettled(busy, idle)).toBe(true);
  });

  it("fires when a transfer finished without ever being seen as busy", () => {
    const before = queueActivity(summary({}));
    const after = queueActivity(summary({ done: 1, total_items: 1 }));
    expect(queueSettled(before, after)).toBe(true);
  });

  it("fires when the queue settles on a failure", () => {
    const busy = queueActivity(summary({ running: 1, total_items: 1 }));
    const idle = queueActivity(summary({ failed: 1, total_items: 1 }));
    expect(queueSettled(busy, idle)).toBe(true);
  });

  it("stays quiet while items are still queued, running, paused or conflicted", () => {
    // Paused and conflicted items are reported inside `running` by the backend.
    const before = queueActivity(summary({ running: 2, total_items: 3 }));
    const after = queueActivity(summary({ running: 1, done: 1, total_items: 3 }));
    expect(queueSettled(before, after)).toBe(false);
  });

  it("stays quiet when finished items are cleared", () => {
    const before = queueActivity(summary({ done: 3, total_items: 3 }));
    const after = queueActivity(summary({}));
    expect(queueSettled(before, after)).toBe(false);
  });

  it("stays quiet on an idle no-change tick", () => {
    const idle = queueActivity(summary({ done: 1, total_items: 1 }));
    expect(queueSettled(idle, idle)).toBe(false);
  });
});
