// @vitest-environment jsdom

import { describe, expect, it, vi } from "vitest";

import { ThemeController, terminalTheme } from "./theme";

class FakeMediaQuery {
  matches: boolean;
  private listeners = new Set<(event: MediaQueryListEvent) => void>();

  constructor(matches: boolean) {
    this.matches = matches;
  }

  addEventListener(_type: "change", listener: (event: MediaQueryListEvent) => void) {
    this.listeners.add(listener);
  }

  removeEventListener(_type: "change", listener: (event: MediaQueryListEvent) => void) {
    this.listeners.delete(listener);
  }

  setMatches(matches: boolean) {
    this.matches = matches;
    const event = { matches } as MediaQueryListEvent;
    for (const listener of this.listeners) listener(event);
  }
}

function createStorage() {
  const values = new Map<string, string>();
  return {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => values.set(key, value),
  };
}

describe("ThemeController", () => {
  it("tracks the OS theme synchronously while System is selected", () => {
    const root = document.createElement("html");
    const media = new FakeMediaQuery(false);
    const setNativeTheme = vi.fn(async () => {});
    const controller = new ThemeController({
      root,
      media: media as unknown as MediaQueryList,
      storage: createStorage(),
      setNativeTheme,
    });
    const observed: string[] = [];

    controller.start("system");
    controller.subscribe((theme) => observed.push(theme));

    expect(root.dataset.theme).toBe("light");
    expect(root.style.colorScheme).toBe("light");
    expect(setNativeTheme).toHaveBeenLastCalledWith(null);

    media.setMatches(true);

    expect(root.dataset.theme).toBe("dark");
    expect(root.style.colorScheme).toBe("dark");
    expect(root.style.transition).toBe("");
    expect(observed).toEqual(["light", "dark"]);
  });

  it("keeps an explicit theme stable across OS changes and caches it", () => {
    const root = document.createElement("html");
    const media = new FakeMediaQuery(true);
    const storage = createStorage();
    const setNativeTheme = vi.fn(async () => {});
    const controller = new ThemeController({
      root,
      media: media as unknown as MediaQueryList,
      storage,
      setNativeTheme,
    });

    controller.start("light");
    media.setMatches(false);

    expect(root.dataset.theme).toBe("light");
    expect(storage.getItem("serverus-theme")).toBe("light");
    expect(setNativeTheme).toHaveBeenLastCalledWith("light");

    controller.setPreference("dark");

    expect(root.dataset.theme).toBe("dark");
    expect(storage.getItem("serverus-theme")).toBe("dark");
    expect(setNativeTheme).toHaveBeenLastCalledWith("dark");
  });

  it("provides readable terminal palettes for both resolved themes", () => {
    expect(terminalTheme("light")).toMatchObject({
      background: "#ffffff",
      foreground: "#1f2328",
    });
    expect(terminalTheme("dark")).toMatchObject({
      background: "#0d1117",
      foreground: "#e6edf3",
    });
  });
});
