// Transfer queue state fed by backend progress events (SPEC §6.1).

import { commands, events, unwrap } from "$lib/api";
import type { TransferSnapshot, TransferSummary } from "$lib/api";

class TransfersStore {
  items = $state<TransferSnapshot[]>([]);
  summary = $state<TransferSummary>({
    queued: 0,
    running: 0,
    done: 0,
    failed: 0,
    total_items: 0,
  });
  collapsed = $state(true);
  private listening = false;

  get active(): boolean {
    return this.summary.queued + this.summary.running > 0;
  }

  get conflicted(): TransferSnapshot | null {
    return this.items.find((i) => i.state === "conflict") ?? null;
  }

  async init() {
    if (this.listening) return;
    this.listening = true;
    await events.transferProgressEvent.listen((e) => {
      this.items = e.payload.items;
      this.summary = e.payload.summary;
      // Auto-open the panel when new work appears.
      if (e.payload.summary.queued + e.payload.summary.running > 0) {
        this.collapsed = false;
      }
    });
  }

  async refresh() {
    const list = await unwrap(commands.transferList());
    this.items = list.items;
    this.summary = list.summary;
  }

  pause = (id: string) => void unwrap(commands.transferPause(id));
  retry = (id: string) => void unwrap(commands.transferRetry(id));
  resume = (id: string) => void unwrap(commands.transferResume(id));
  cancel = (id: string) => void unwrap(commands.transferCancel(id));
  pauseAll = () => void unwrap(commands.transferPauseAll());
  resumeAll = () => void unwrap(commands.transferResumeAll());
  cancelAll = () => void unwrap(commands.transferCancelAll());

  async clearFinished() {
    await unwrap(commands.transferClearFinished());
    await this.refresh();
  }

  async resolve(
    sessionId: string,
    id: string,
    action: "overwrite" | "skip" | "rename",
    applyToAll: boolean,
  ) {
    await unwrap(commands.transferResolve(sessionId, id, action, applyToAll));
  }
}

export const transfers = new TransfersStore();
