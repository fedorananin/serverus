// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { PublicVault, Settings } from "$lib/api";
import { vault } from "$lib/stores/vault.svelte";
import { DEFAULT_COLUMN_WIDTHS, FileColumnWidths } from "./column-widths.svelte";

function press(clientX: number) {
  const handle = document.createElement("span");
  handle.setPointerCapture = vi.fn();
  return {
    button: 0,
    clientX,
    pointerId: 1,
    currentTarget: handle,
    preventDefault() {},
    stopPropagation() {},
  } as unknown as PointerEvent;
}

const move = (clientX: number) => window.dispatchEvent(new MouseEvent("pointermove", { clientX }));
const release = () => window.dispatchEvent(new MouseEvent("pointerup"));

let saved: Settings[];

beforeEach(() => {
  saved = [];
  vault.data = {
    settings: { panels: { columns: { ...DEFAULT_COLUMN_WIDTHS } } },
  } as unknown as PublicVault;
  vi.spyOn(vault, "updateSettings").mockImplementation(async (settings) => {
    saved.push(settings);
  });
});

afterEach(() => {
  vault.data = null;
  vi.restoreAllMocks();
});

describe("file pane column resizing", () => {
  it("moves the column's left edge with the pointer and saves once on release", async () => {
    const columns = new FileColumnWidths();
    columns.startResize("date", press(500));

    move(470);
    move(440);
    expect(columns.current.date).toBe(DEFAULT_COLUMN_WIDTHS.date + 60);
    expect(columns.style).toContain(`--col-date: ${DEFAULT_COLUMN_WIDTHS.date + 60}px`);
    expect(saved).toHaveLength(0);

    release();
    await vi.waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0].panels.columns).toEqual({
      ...DEFAULT_COLUMN_WIDTHS,
      date: DEFAULT_COLUMN_WIDTHS.date + 60,
    });
  });

  it("keeps widths within the backend's bounds", async () => {
    const columns = new FileColumnWidths();
    columns.startResize("size", press(500));
    move(5000);
    expect(columns.current.size).toBe(40);
    move(-5000);
    expect(columns.current.size).toBe(320);
    release();
    await vi.waitFor(() => expect(saved).toHaveLength(1));
  });

  it("does not write the vault for a press without movement", async () => {
    const columns = new FileColumnWidths();
    columns.startResize("mode", press(500));
    release();
    await Promise.resolve();
    expect(saved).toHaveLength(0);
  });

  it("resets one column to its default on double-click", async () => {
    vault.data!.settings.panels.columns = { size: 120, date: 200, mode: 60 };
    const columns = new FileColumnWidths();
    columns.reset("date");
    await vi.waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0].panels.columns).toEqual({
      size: 120,
      date: DEFAULT_COLUMN_WIDTHS.date,
      mode: 60,
    });
  });
});
