// Transfer queue state fed by backend progress events (SPEC §6.1).

import type {
  AppApi,
  ConflictAction,
  TransferListDto,
  TransferSnapshot,
  TransferSummary,
} from "$lib/app/contracts/api";
import type { AppEventSource } from "$lib/app/contracts/events";

function emptySummary(): TransferSummary {
  return {
    queued: 0,
    running: 0,
    done: 0,
    failed: 0,
    total_items: 0,
  };
}

export class TransfersStore {
  items = $state<TransferSnapshot[]>([]);
  summary = $state<TransferSummary>(emptySummary());
  collapsed = $state(true);
  private subscriptionInitialization: Promise<void> | null = null;
  private contextInitialization: { epoch: number; promise: Promise<void> } | null = null;
  private contextEpoch = 0;
  private bootstrappedEpoch: number | null = null;
  private progressRevision = 0;
  private bufferedProgress: { epoch: number; snapshot: TransferListDto } | null = null;
  private activeContextId: string | null = null;
  private readonly retiredContextIds = new Set<string>();

  constructor(
    private readonly api: AppApi,
    private readonly eventSource: AppEventSource,
  ) {}

  get active(): boolean {
    return this.summary.queued + this.summary.running > 0;
  }

  get conflicted(): TransferSnapshot | null {
    return this.items.find((i) => i.state === "conflict") ?? null;
  }

  init(): Promise<void> {
    const epoch = this.contextEpoch;
    if (this.contextInitialization?.epoch !== epoch) {
      this.contextInitialization = { epoch, promise: this.initialize(epoch) };
    }
    return this.contextInitialization.promise;
  }

  private async initialize(epoch: number) {
    this.subscriptionInitialization ??= this.eventSource.transfers
      .listenProgress((snapshot) => {
        if (!this.canAcceptSnapshot(snapshot)) return;
        this.progressRevision += 1;
        if (this.bootstrappedEpoch !== this.contextEpoch) {
          this.bufferedProgress = { epoch: this.contextEpoch, snapshot };
          return;
        }
        this.applyProgress(snapshot);
      })
      .then(() => undefined);
    await this.subscriptionInitialization;
    await this.refreshForEpoch(epoch);
  }

  private applySnapshot(snapshot: TransferListDto): boolean {
    if (!this.canAcceptSnapshot(snapshot)) return false;
    this.activeContextId = snapshot.runtime_context_id;
    this.items = snapshot.items;
    this.summary = snapshot.summary;
    return true;
  }

  private canAcceptSnapshot(snapshot: TransferListDto): boolean {
    const contextId = snapshot.runtime_context_id;
    return (
      !this.retiredContextIds.has(contextId) &&
      (this.activeContextId === null || this.activeContextId === contextId)
    );
  }

  private applyProgress(snapshot: TransferListDto): boolean {
    if (!this.applySnapshot(snapshot)) return false;
    if (snapshot.summary.queued + snapshot.summary.running > 0) this.collapsed = false;
    return true;
  }

  async refresh() {
    await this.refreshForEpoch(this.contextEpoch);
  }

  private async refreshForEpoch(epoch: number) {
    const progressRevision = this.progressRevision;
    const snapshot = await this.api.transfers.list();
    if (epoch !== this.contextEpoch) return;
    if (this.progressRevision !== progressRevision) {
      const buffered = this.bufferedProgress;
      if (buffered?.epoch === epoch && this.applyProgress(buffered.snapshot)) {
        this.bufferedProgress = null;
        this.bootstrappedEpoch = epoch;
      }
      return;
    }
    if (this.bufferedProgress?.epoch === epoch) this.bufferedProgress = null;
    if (this.applySnapshot(snapshot)) this.bootstrappedEpoch = epoch;
  }

  /** Clear the cached view after the backend runtime has retired. The
   *  app-lifetime event subscription stays installed for the next context. */
  retireContext() {
    this.contextEpoch += 1;
    this.contextInitialization = null;
    this.bootstrappedEpoch = null;
    this.bufferedProgress = null;
    if (this.activeContextId !== null) this.retiredContextIds.add(this.activeContextId);
    this.activeContextId = null;
    this.items = [];
    this.summary = emptySummary();
    this.collapsed = true;
  }

  upload(sessionId: string, localPath: string, remoteDir: string): Promise<void> {
    return this.api.transfers.upload(sessionId, localPath, remoteDir);
  }

  download(sessionId: string, remotePath: string, localDir: string): Promise<void> {
    return this.api.transfers.download(sessionId, remotePath, localDir);
  }

  pause = (id: string) => void this.api.transfers.pause(id);
  retry = (id: string) => void this.api.transfers.retry(id);
  resume = (id: string) => void this.api.transfers.resume(id);
  cancel = (id: string) => void this.api.transfers.cancel(id);
  pauseAll = () => this.applyToActiveContext((id) => this.api.transfers.pauseAll(id));
  resumeAll = () => this.applyToActiveContext((id) => this.api.transfers.resumeAll(id));
  cancelAll = () => this.applyToActiveContext((id) => this.api.transfers.cancelAll(id));

  private applyToActiveContext(action: (contextId: string) => Promise<void>): Promise<void> {
    return this.activeContextId === null ? Promise.resolve() : action(this.activeContextId);
  }

  async clearFinished() {
    const contextId = this.activeContextId;
    if (contextId === null) return;
    await this.api.transfers.clearFinished(contextId);
    await this.refresh();
  }

  async resolve(
    sessionId: string,
    id: string,
    action: ConflictAction,
    applyToAll: boolean,
  ) {
    await this.api.transfers.resolve(sessionId, id, action, applyToAll);
  }
}
