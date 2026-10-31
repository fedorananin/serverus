// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";

import { hasOpenModal, isInTopLayer, modalLayer } from "./modal-stack";

const opened: { destroy(): void }[] = [];

function dialog(options: Parameters<typeof modalLayer>[1], ...buttons: string[]) {
  const node = document.createElement("div");
  node.tabIndex = -1;
  for (const label of buttons) {
    const button = document.createElement("button");
    button.textContent = label;
    node.appendChild(button);
  }
  document.body.appendChild(node);
  const layer = modalLayer(node, options);
  opened.push(layer);
  return { node, layer, button: (i: number) => node.querySelectorAll("button")[i] };
}

const press = (key: string, init: KeyboardEventInit = {}) =>
  window.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, ...init }));

afterEach(() => {
  for (const layer of opened.splice(0)) layer.destroy();
  document.body.innerHTML = "";
});

describe("modal stack", () => {
  it("lets only the topmost dialog hear Escape", () => {
    const outer = vi.fn();
    const inner = vi.fn();
    dialog({ onescape: outer });
    const top = dialog({ onescape: inner });

    press("Escape");
    expect(inner).toHaveBeenCalledOnce();
    expect(outer).not.toHaveBeenCalled();

    top.layer.destroy();
    press("Escape");
    expect(outer).toHaveBeenCalledOnce();
  });

  it("keeps Escape from reaching other window listeners", () => {
    const behind = vi.fn();
    window.addEventListener("keydown", behind);
    dialog({ onescape: vi.fn() });
    press("Escape");
    window.removeEventListener("keydown", behind);
    expect(behind).not.toHaveBeenCalled();
  });

  it("swallows Escape on a dialog that must be answered", () => {
    const below = vi.fn();
    dialog({ onescape: below });
    dialog({}); // a host-key prompt: no Escape
    press("Escape");
    expect(below).not.toHaveBeenCalled();
  });

  it("wraps Tab inside a focus-trapping dialog", () => {
    const { button } = dialog({ trapFocus: true }, "first", "last");
    button(1).focus();
    press("Tab");
    expect(document.activeElement).toBe(button(0));
    press("Tab", { shiftKey: true });
    expect(document.activeElement).toBe(button(1));
  });

  it("leaves Tab alone in a dialog that pops up unasked", () => {
    const terminal = document.createElement("textarea");
    document.body.appendChild(terminal);
    terminal.focus();
    dialog({}, "Allow once");
    press("Tab");
    expect(document.activeElement).toBe(terminal);
  });

  it("gives focus back to where it was when the dialog closes", () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    const { layer, node } = dialog({ trapFocus: true });
    expect(document.activeElement).toBe(node);

    layer.destroy();
    expect(document.activeElement).toBe(opener);
  });

  it("reports whether a dialog is open and which one is on top", () => {
    expect(hasOpenModal()).toBe(false);
    const below = dialog({});
    const top = dialog({}, "ok");
    expect(hasOpenModal()).toBe(true);
    expect(isInTopLayer(top.button(0))).toBe(true);
    expect(isInTopLayer(below.node)).toBe(false);
  });
});
