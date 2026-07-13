// Platform detection for keyboard/mouse conventions. The webview is WKWebView
// on macOS, WebView2 on Windows, WebKitGTK on Linux — navigator reports the
// host OS in all three.

export const isMac = /mac/i.test(navigator.platform || navigator.userAgent);

/** The primary "command" modifier: ⌘ on macOS, Ctrl elsewhere. */
export function isMod(e: KeyboardEvent | MouseEvent | PointerEvent): boolean {
  return isMac ? e.metaKey : e.ctrlKey;
}

/** Human-readable name of the primary modifier, for hints and labels. */
export const modKey = isMac ? "⌘" : "Ctrl+";
