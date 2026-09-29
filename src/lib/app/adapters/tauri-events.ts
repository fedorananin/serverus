import { events } from "$lib/api";
import type {
  AgentEventSource,
  AppEventSource,
  RemoteEditEventSource,
  TransferEventSource,
} from "$lib/app/contracts/events";

export class TauriAppEventSource implements AppEventSource {
  readonly transfers: TransferEventSource = {
    listenProgress: (listener) =>
      events.transferProgressEvent.listen((event) => listener(event.payload)),
  };

  readonly remoteEdit: RemoteEditEventSource = {
    listenUploaded: (listener) =>
      events.remoteEditUploadedEvent.listen((event) => listener(event.payload)),
  };

  readonly agent: AgentEventSource = {
    listenUiRequests: (listener) =>
      events.agentUiRequestEvent.listen((event) => listener(event.payload)),
    listenUiExpired: (listener) =>
      events.agentUiRequestExpiredEvent.listen((event) => listener(event.payload)),
    listenTerminal: (listener) =>
      events.agentTerminalEvent.listen((event) => listener(event.payload)),
    listenActivity: (listener) =>
      events.agentActivityEvent.listen((event) => listener(event.payload)),
    listenFsChanged: (listener) =>
      events.agentFsChangedEvent.listen((event) => listener(event.payload)),
    listenVaultChanged: (listener) =>
      events.agentVaultChangedEvent.listen((event) => listener(event.payload)),
  };
}
