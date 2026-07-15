import { setTheme as setNativeTheme } from "@tauri-apps/api/app";
import type { ITheme } from "@xterm/xterm";

import type { ThemePreference } from "$lib/api";

export type ResolvedTheme = Exclude<ThemePreference, "system">;

const STORAGE_KEY = "serverus-theme";
const SYSTEM_QUERY = "(prefers-color-scheme: dark)";

interface ThemeStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

interface ThemeEnvironment {
  root: HTMLElement;
  media: MediaQueryList;
  storage: ThemeStorage | null;
  setNativeTheme: (theme: ResolvedTheme | null) => Promise<void>;
}

type ThemeListener = (theme: ResolvedTheme) => void;

function isThemePreference(value: string | null): value is ThemePreference {
  return value === "system" || value === "light" || value === "dark";
}

export class ThemeController {
  private preference: ThemePreference = "system";
  private resolvedTheme: ResolvedTheme;
  private started = false;
  private listeners = new Set<ThemeListener>();

  constructor(private readonly environment: ThemeEnvironment) {
    this.resolvedTheme = environment.media.matches ? "dark" : "light";
  }

  private readonly onSystemThemeChange = () => {
    if (this.preference === "system") this.applyResolvedTheme();
  };

  start(initialPreference?: ThemePreference) {
    if (this.started) return;
    this.started = true;
    this.environment.media.addEventListener("change", this.onSystemThemeChange);
    this.setPreference(initialPreference ?? this.readCachedPreference(), true);
  }

  stop() {
    if (!this.started) return;
    this.environment.media.removeEventListener("change", this.onSystemThemeChange);
    this.started = false;
  }

  setPreference(preference: ThemePreference, force = false) {
    if (!force && this.started && preference === this.preference) return;
    this.preference = preference;
    this.writeCachedPreference(preference);
    this.applyResolvedTheme();
    void this.environment.setNativeTheme(preference === "system" ? null : preference).catch(() => {
      // CSS still follows the requested theme if the native bridge is unavailable.
    });
  }

  get resolved(): ResolvedTheme {
    return this.resolvedTheme;
  }

  subscribe(listener: ThemeListener): () => void {
    this.listeners.add(listener);
    listener(this.resolvedTheme);
    return () => this.listeners.delete(listener);
  }

  private applyResolvedTheme() {
    const next = this.preference === "system"
      ? this.environment.media.matches ? "dark" : "light"
      : this.preference;
    const changed = next !== this.resolvedTheme;
    this.resolvedTheme = next;
    this.environment.root.dataset.theme = next;
    this.environment.root.dataset.themePreference = this.preference;
    this.environment.root.style.colorScheme = next;
    if (changed) {
      for (const listener of this.listeners) listener(next);
    }
  }

  private readCachedPreference(): ThemePreference {
    try {
      const value = this.environment.storage?.getItem(STORAGE_KEY) ?? null;
      return isThemePreference(value) ? value : "system";
    } catch {
      return "system";
    }
  }

  private writeCachedPreference(preference: ThemePreference) {
    try {
      this.environment.storage?.setItem(STORAGE_KEY, preference);
    } catch {
      // Theme persistence is a convenience; vault settings remain authoritative.
    }
  }
}

let browserController: ThemeController | null = null;

function fallbackMediaQuery(): MediaQueryList {
  return {
    matches: false,
    media: SYSTEM_QUERY,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  };
}

function appTheme(): ThemeController {
  if (!browserController) {
    const media = window.matchMedia?.(SYSTEM_QUERY) ?? fallbackMediaQuery();
    browserController = new ThemeController({
      root: document.documentElement,
      media,
      storage: window.localStorage,
      setNativeTheme,
    });
  }
  return browserController;
}

export function initializeTheme() {
  appTheme().start();
}

export function setThemePreference(preference: ThemePreference) {
  const controller = appTheme();
  controller.start();
  controller.setPreference(preference);
}

export function subscribeResolvedTheme(listener: ThemeListener): () => void {
  const controller = appTheme();
  controller.start();
  return controller.subscribe(listener);
}

export function currentResolvedTheme(): ResolvedTheme {
  const controller = appTheme();
  controller.start();
  return controller.resolved;
}

export function terminalTheme(theme: ResolvedTheme): ITheme {
  if (theme === "light") {
    return {
      background: "#ffffff",
      foreground: "#1f2328",
      cursor: "#1f883d",
      cursorAccent: "#ffffff",
      selectionBackground: "rgba(31, 136, 61, 0.24)",
      black: "#24292f",
      red: "#cf222e",
      green: "#116329",
      yellow: "#9a6700",
      blue: "#0969da",
      magenta: "#8250df",
      cyan: "#1b7c83",
      white: "#6e7781",
      brightBlack: "#57606a",
      brightRed: "#a40e26",
      brightGreen: "#1a7f37",
      brightYellow: "#7d4e00",
      brightBlue: "#0550ae",
      brightMagenta: "#6639ba",
      brightCyan: "#096b72",
      brightWhite: "#1f2328",
    };
  }
  return {
    background: "#0d1117",
    foreground: "#e6edf3",
    cursor: "#3fb950",
    cursorAccent: "#0d1117",
    selectionBackground: "rgba(63, 185, 80, 0.3)",
  };
}
