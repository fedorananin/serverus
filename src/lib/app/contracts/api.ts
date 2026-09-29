import type {
  AgentActivityEntry,
  AgentConfirmDecision,
  AgentSetupInfo,
  AgentTabInfo,
  AgentTerminalEvent,
  AgentUiRequest,
  AgentUiResponse,
  ChmodScope,
  ConflictAction,
  TransferListDto,
  TransferSnapshot,
  TransferSummary,
  TreeTarget,
} from "$lib/api";

export type {
  AgentActivityEntry,
  AgentConfirmDecision,
  AgentSetupInfo,
  AgentTabInfo,
  AgentTerminalEvent,
  AgentUiRequest,
  AgentUiResponse,
  ChmodScope,
  ConflictAction,
  TransferListDto,
  TransferSnapshot,
  TransferSummary,
  TreeTarget,
};

export interface TransfersApi {
  list(): Promise<TransferListDto>;
  /** One call per user action: the whole selection shares one conflict batch. */
  upload(sessionId: string, localPaths: string[], remoteDir: string): Promise<void>;
  download(sessionId: string, remotePaths: string[], localDir: string): Promise<void>;
  /** Queue a recursive delete per target; progress shows up in the panel. */
  delete(sessionId: string, targets: TreeTarget[]): Promise<void>;
  /** Queue a recursive chmod per target directory. */
  chmod(sessionId: string, targets: TreeTarget[], mode: number, scope: ChmodScope): Promise<void>;
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

/** AI agent (MCP) bridge: answering the backend's UI requests and
 *  handing terminals between the user and the agent. */
export interface AgentApi {
  /** Resolves false when the request already expired. */
  respond(requestId: string, response: AgentUiResponse): Promise<boolean>;
  takeOver(termId: string): Promise<void>;
  handBack(termId: string): Promise<void>;
  terminalStates(): Promise<AgentTerminalEvent[]>;
  setupInfo(): Promise<AgentSetupInfo>;
}

/** Frontend-facing command boundary, extended one feature namespace at a time. */
export interface AppApi {
  transfers: TransfersApi;
  vault: VaultActivityApi;
  agent: AgentApi;
}
