import type { RemoteEditUploadedEvent } from "$lib/api";
import type { TransferListDto } from "./api";

export type AppUnlisten = () => void;

export interface TransferEventSource {
  listenProgress(listener: (snapshot: TransferListDto) => void): Promise<AppUnlisten>;
}

export interface RemoteEditEventSource {
  listenUploaded(listener: (event: RemoteEditUploadedEvent) => void): Promise<AppUnlisten>;
}

/** Frontend-facing event boundary, extended one feature namespace at a time. */
export interface AppEventSource {
  transfers: TransferEventSource;
  remoteEdit: RemoteEditEventSource;
}
