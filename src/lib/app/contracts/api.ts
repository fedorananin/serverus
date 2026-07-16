import type {
  ConflictAction,
  TransferListDto,
  TransferSnapshot,
  TransferSummary,
} from "$lib/api";

export type { ConflictAction, TransferListDto, TransferSnapshot, TransferSummary };

export interface TransfersApi {
  list(): Promise<TransferListDto>;
  /** One call per user action: the whole selection shares one conflict batch. */
  upload(sessionId: string, localPaths: string[], remoteDir: string): Promise<void>;
  download(sessionId: string, remotePaths: string[], localDir: string): Promise<void>;
  pause(id: string): Promise<void>;
  retry(id: string): Promise<void>;
  resume(id: string): Promise<void>;
  cancel(id: string): Promise<void>;
  /** Bulk actions are per session: each tab drives only its own transfers. */
  pauseAll(runtimeContextId: string, sessionId: string): Promise<void>;
  resumeAll(runtimeContextId: string, sessionId: string): Promise<void>;
  cancelAll(runtimeContextId: string, sessionId: string): Promise<void>;
  clearFinished(runtimeContextId: string, sessionId: string): Promise<void>;
  resolve(
    sessionId: string,
    id: string,
    action: ConflictAction,
    applyToAll: boolean,
  ): Promise<void>;
}

export interface VaultActivityApi {
  touchActivity(): Promise<void>;
}

/** Frontend-facing command boundary, extended one feature namespace at a time. */
export interface AppApi {
  transfers: TransfersApi;
  vault: VaultActivityApi;
}
