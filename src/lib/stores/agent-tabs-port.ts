// Adapts the legacy tabs store to the port the agent store works against.

import type { AgentTabsPort } from "./agent-tabs";
import type { Tab } from "./tabs.svelte";

interface TabsSource {
  readonly tabs: Tab[];
  readonly activeId: string | null;
  open(connectionId: string, options?: { background?: boolean }): Tab;
  connect(tabId: string): Promise<void>;
}

export function agentTabsPort(tabs: TabsSource): AgentTabsPort {
  return {
    list: () =>
      tabs.tabs.map((tab) => ({
        tabId: tab.id,
        connectionId: tab.connectionId,
        sessionId: tab.sessionId,
        state: tab.state,
        error: tab.error,
        active: tab.id === tabs.activeId,
      })),
    // The agent never pulls the user away from the tab they are in.
    open: (connectionId) => tabs.open(connectionId, { background: true }).id,
    reconnect: (tabId) => void tabs.connect(tabId),
  };
}
