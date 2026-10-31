// Every open dialog is a layer. Only the topmost one hears Escape (and,
// for dialogs the user opened, keeps Tab inside itself), and app-wide
// shortcuts stay off while any layer is up: ⌘W must not close the tab
// under a dialog, and one Escape must not close a whole stack of them.
//
// Each dialog's own window listener can't do this: all of them sit on the
// same `window`, so `stopPropagation` doesn't reach the others, and the
// outer dialog — registered first — would run first anyway. One capture
// listener decides instead.

export interface ModalLayerOptions {
  /** Escape on the topmost layer. Without it, Escape is just swallowed. */
  onescape?: () => void;
  /** Keep Tab inside, focus into the dialog on open, give focus back on
   *  close. Off for dialogs that pop up unasked (agent confirmations, host
   *  keys) — those must never take the keyboard from a terminal mid-typing. */
  trapFocus?: boolean;
}

interface Layer {
  node: HTMLElement;
  options: ModalLayerOptions;
}

const layers: Layer[] = [];

// Recent focus targets, newest last, to give focus back on close. Tracked
// rather than read at open time: a dialog's children may already have
// focused themselves (an input, a confirm button) before it registers.
const recentFocus: Element[] = [];
if (typeof window !== "undefined") {
  window.addEventListener(
    "focusin",
    (event) => {
      if (!(event.target instanceof Element)) return;
      recentFocus.push(event.target);
      if (recentFocus.length > 8) recentFocus.shift();
    },
    true,
  );
}

/** Whether any dialog is open — app-wide shortcuts check this. */
export function hasOpenModal(): boolean {
  return layers.length > 0;
}

/** Whether `element` belongs to the topmost dialog — for a dialog's own
 *  window-level keys (Enter = confirm) that must not fire from under
 *  another one. */
export function isInTopLayer(element: Element | undefined): boolean {
  const top = layers.at(-1);
  return element !== undefined && top !== undefined && top.node.contains(element);
}

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])';

function focusables(node: HTMLElement): HTMLElement[] {
  return Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE)).filter(
    (element) => !element.closest("[inert]"),
  );
}

function onKeydown(event: KeyboardEvent) {
  const top = layers.at(-1);
  if (!top) return;
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    top.options.onescape?.();
  } else if (event.key === "Tab" && top.options.trapFocus) {
    const items = focusables(top.node);
    if (items.length === 0) {
      event.preventDefault();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    const active = document.activeElement;
    const inside = active instanceof Node && top.node.contains(active);
    if (event.shiftKey && (!inside || active === first)) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && (!inside || active === last)) {
      event.preventDefault();
      first.focus();
    }
  }
}

/** Svelte action: `<div use:modalLayer={{ onescape: close, trapFocus: true }}>`. */
export function modalLayer(node: HTMLElement, options: ModalLayerOptions = {}) {
  const layer: Layer = { node, options };
  if (layers.length === 0) window.addEventListener("keydown", onKeydown, true);
  layers.push(layer);

  const active = document.activeElement;
  const focusedInside = active instanceof Node && node.contains(active);
  const previous = focusedInside
    ? ([...recentFocus].reverse().find((element) => !node.contains(element)) ?? null)
    : active;
  if (options.trapFocus && !focusedInside) node.focus();

  return {
    update(next: ModalLayerOptions = {}) {
      layer.options = next;
    },
    destroy() {
      const index = layers.indexOf(layer);
      if (index >= 0) layers.splice(index, 1);
      if (layers.length === 0) window.removeEventListener("keydown", onKeydown, true);
      if (
        options.trapFocus &&
        previous instanceof HTMLElement &&
        previous.isConnected &&
        !layers.some((other) => other.node.contains(previous))
      ) {
        previous.focus();
      }
    },
  };
}
