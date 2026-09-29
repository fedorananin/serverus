import { getContext, setContext } from "svelte";
import { TauriAppApi } from "./adapters/tauri-api";
import { TauriAppEventSource } from "./adapters/tauri-events";
import type { AppApi } from "./contracts/api";
import type { AppEventSource } from "./contracts/events";
import { TransfersStore } from "$lib/stores/transfers.svelte";
import { AgentStore } from "$lib/stores/agent.svelte";

const appModelContext = Symbol("serverus-app-model");

export class AppModel {
  readonly transfers: TransfersStore;
  readonly agent: AgentStore;

  constructor(
    readonly api: AppApi,
    readonly eventSource: AppEventSource,
  ) {
    this.transfers = new TransfersStore(api, eventSource);
    this.agent = new AgentStore(api.agent, eventSource.agent);
  }
}

export function createAppModel(api: AppApi, eventSource: AppEventSource): AppModel {
  return new AppModel(api, eventSource);
}

export function createTauriAppModel(): AppModel {
  return createAppModel(new TauriAppApi(), new TauriAppEventSource());
}

interface ContextRetirementSource {
  onContextRetired(handler: () => void): () => void;
}

interface AccessRevocationSource {
  onAccessRevoked(handler: () => void): () => void;
}

interface TabsContextLifecycle {
  retireContext(): void;
}

interface HostKeyContextLifecycle {
  clearForContextRetirement(): void;
}

interface HostKeyAccessLifecycle {
  clearForAccessRevocation(): void;
}

interface TransfersContextLifecycle {
  retireContext(): void;
}

export function wireContextRetirement(
  source: ContextRetirementSource,
  tabs: TabsContextLifecycle,
  hostKey: HostKeyContextLifecycle,
  transfers: TransfersContextLifecycle,
): () => void {
  return source.onContextRetired(() => {
    tabs.retireContext();
    hostKey.clearForContextRetirement();
    transfers.retireContext();
  });
}

export function wireAccessRevocation(
  source: AccessRevocationSource,
  hostKey: HostKeyAccessLifecycle,
): () => void {
  return source.onAccessRevoked(() => hostKey.clearForAccessRevocation());
}

export function provideAppModel(model: AppModel): void {
  setContext(appModelContext, model);
}

/** For components that also render outside the app (isolated tests) and
 *  simply skip app-scoped features there. */
export function useOptionalAppModel(): AppModel | null {
  return getContext<AppModel | undefined>(appModelContext) ?? null;
}

export function useAppModel(): AppModel {
  const model = getContext<AppModel | undefined>(appModelContext);
  if (!model) throw new Error("AppModel is not available in Svelte context");
  return model;
}
