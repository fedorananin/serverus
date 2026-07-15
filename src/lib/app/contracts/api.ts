import type {
  ConflictAction,
  TransferListDto,
  TransferSnapshot,
  TransferSummary,
} from "$lib/api";

export type { ConflictAction, TransferListDto, TransferSnapshot, TransferSummary };

export interface TransfersApi {
  list(): Promise<TransferListDto>;
  upload(sessionId: string, localPath: string, remoteDir: string): Promise<void>;
  download(sessionId: string, remotePath: string, localDir: string): Promise<void>;
  pause(id: string): Promise<void>;
  retry(id: string): Promise<void>;
  resume(id: string): Promise<void>;
  cancel(id: string): Promise<void>;
  pauseAll(runtimeContextId: string): Promise<void>;
  resumeAll(runtimeContextId: string): Promise<void>;
  cancelAll(runtimeContextId: string): Promise<void>;
  clearFinished(runtimeContextId: string): Promise<void>;
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
