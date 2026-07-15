import { events } from "$lib/api";
import type {
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
}
