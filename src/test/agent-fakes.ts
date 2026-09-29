// Inert agent ports for tests whose subject is not the agent feature.

import type { AgentApi } from "$lib/app/contracts/api";
import type { AgentEventSource } from "$lib/app/contracts/events";

export function inertAgentApi(): AgentApi {
  return {
    respond: async () => true,
    takeOver: async () => {},
    handBack: async () => {},
    terminalStates: async () => [],
    setupInfo: async () => ({
      supported: true,
      listening: true,
      problem: null,
      command: "/Applications/Serverus.app/Contents/MacOS/serverus",
      args: ["--mcp"],
      claude_code: "claude mcp add --scope user serverus -- serverus --mcp",
      command_warning: null,
    }),
  };
}

export function inertAgentEvents(): AgentEventSource {
  const none = async () => () => {};
  return {
    listenUiRequests: none,
    listenUiExpired: none,
    listenTerminal: none,
    listenActivity: none,
    listenFsChanged: none,
    listenVaultChanged: none,
  };
}
