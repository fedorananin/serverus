import type {
  AgentActivityEvent,
  AgentFsChangedEvent,
  AgentTerminalEvent,
  AgentUiRequestEvent,
  AgentUiRequestExpiredEvent,
  AgentVaultChangedEvent,
  RemoteEditUploadedEvent,
} from "$lib/api";
import type { TransferListDto } from "./api";

export type {
  AgentActivityEvent,
  AgentFsChangedEvent,
  AgentUiRequestEvent,
  AgentUiRequestExpiredEvent,
  AgentVaultChangedEvent,
};

export type AppUnlisten = () => void;

export interface TransferEventSource {
  listenProgress(listener: (snapshot: TransferListDto) => void): Promise<AppUnlisten>;
}

export interface RemoteEditEventSource {
  listenUploaded(listener: (event: RemoteEditUploadedEvent) => void): Promise<AppUnlisten>;
}

export interface AgentEventSource {
  listenUiRequests(listener: (event: AgentUiRequestEvent) => void): Promise<AppUnlisten>;
  /** A request ended unanswered (timed out, or the agent cancelled it). */
  listenUiExpired(listener: (event: AgentUiRequestExpiredEvent) => void): Promise<AppUnlisten>;
  listenTerminal(listener: (event: AgentTerminalEvent) => void): Promise<AppUnlisten>;
  listenActivity(listener: (event: AgentActivityEvent) => void): Promise<AppUnlisten>;
  listenFsChanged(listener: (event: AgentFsChangedEvent) => void): Promise<AppUnlisten>;
  listenVaultChanged(listener: (event: AgentVaultChangedEvent) => void): Promise<AppUnlisten>;
}

/** Frontend-facing event boundary, extended one feature namespace at a time. */
export interface AppEventSource {
  transfers: TransferEventSource;
  remoteEdit: RemoteEditEventSource;
  agent: AgentEventSource;
}
