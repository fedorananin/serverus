import type { Terminal } from "@xterm/xterm";

import {
  currentResolvedTheme,
  subscribeResolvedTheme,
  terminalTheme,
} from "$lib/theme";

export function terminalThemeOptions() {
  return terminalTheme(currentResolvedTheme());
}

export function syncTerminalTheme(terminal: Terminal): () => void {
  return subscribeResolvedTheme((theme) => {
    terminal.options.theme = terminalTheme(theme);
  });
}
