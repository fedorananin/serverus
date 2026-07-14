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
  private contextEpoch: number | null = null;

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
      if (e.payload.context_epoch !== this.contextEpoch) return;
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
    if (list.context_epoch !== this.contextEpoch) return;
    this.items = list.items;
    this.summary = list.summary;
  }

  pause = (id: string) => {
    if (this.contextEpoch !== null)
      void unwrap(commands.transferPause(id));
  };
  retry = (id: string) => {
    if (this.contextEpoch !== null)
      void unwrap(commands.transferRetry(id));
  };
  resume = (id: string) => {
    if (this.contextEpoch !== null)
      void unwrap(commands.transferResume(id));
  };
  cancel = (id: string) => {
    if (this.contextEpoch !== null)
      void unwrap(commands.transferCancel(id));
  };
  pauseAll = () => {
    if (this.contextEpoch !== null)
      void unwrap(commands.transferPauseAll());
  };
  resumeAll = () => {
    if (this.contextEpoch !== null)
      void unwrap(commands.transferResumeAll());
  };
  cancelAll = () => {
    if (this.contextEpoch !== null)
      void unwrap(commands.transferCancelAll());
  };

  async clearFinished() {
    if (this.contextEpoch === null) return;
    await unwrap(commands.transferClearFinished());
    await this.refresh();
  }

  resetVaultContext() {
    this.contextEpoch = null;
    this.items = [];
    this.summary = {
      queued: 0,
      running: 0,
      done: 0,
      failed: 0,
      total_items: 0,
    };
    this.collapsed = true;
  }

  setVaultContext(contextEpoch: number) {
    this.contextEpoch = contextEpoch;
  }

  async resolve(
    sessionId: string,
    id: string,
    action: "overwrite" | "skip" | "rename",
    applyToAll: boolean,
  ) {
    if (this.contextEpoch === null) return;
    await unwrap(
      commands.transferResolve(sessionId, id, action, applyToAll),
    );
  }
}

export const transfers = new TransfersStore();
