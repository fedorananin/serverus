import { isMod } from "$lib/platform";
import type { PaneController } from "$lib/stores/pane.svelte";
import { FILE_ROW_HEIGHT } from "./types";

export interface MarqueeRect {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

export class FilePaneMarquee {
  rect = $state<MarqueeRect | null>(null);
  justEnded = false;

  private from: { x: number; y: number } | null = null;
  private base: Set<string> | null = null;
  private pointer = { x: 0, y: 0 };
  private raf = 0;

  constructor(
    private readonly getPane: () => PaneController,
    private readonly getScroller: () => HTMLDivElement | undefined,
    private readonly onScroll: (scrollTop: number) => void,
  ) {}

  isRowWhitespace(event: PointerEvent): boolean {
    const row = (event.target as HTMLElement).closest?.(".row");
    const name = row?.querySelector(".cell.name");
    if (!name) return true;
    const range = document.createRange();
    range.selectNodeContents(name);
    return event.clientX > range.getBoundingClientRect().right + 6;
  }

  pointerDown(event: PointerEvent) {
    const scroller = this.getScroller();
    if (event.button !== 0 || !scroller) return;
    scroller.focus();
    if ((event.target as HTMLElement).closest?.(".pane-error")) return;
    const bounds = scroller.getBoundingClientRect();
    if (event.clientX - bounds.left > scroller.clientWidth) return;
    if (!this.isRowWhitespace(event)) return;
    this.from = {
      x: event.clientX - bounds.left,
      y: event.clientY - bounds.top + scroller.scrollTop,
    };
    const pane = this.getPane();
    this.base = isMod(event) || event.shiftKey ? new Set(pane.selected) : null;
    this.pointer = { x: event.clientX, y: event.clientY };
    scroller.setPointerCapture(event.pointerId);
    window.addEventListener("pointermove", this.onMove);
    window.addEventListener("pointerup", this.onUp);
  }

  private readonly onMove = (event: PointerEvent) => {
    this.pointer = { x: event.clientX, y: event.clientY };
    this.update();
    if (!this.raf) this.raf = requestAnimationFrame(this.tick);
  };

  private readonly tick = () => {
    this.raf = 0;
    const scroller = this.getScroller();
    if (!this.from || !scroller) return;
    const bounds = scroller.getBoundingClientRect();
    const delta =
      this.pointer.y < bounds.top
        ? this.pointer.y - bounds.top
        : this.pointer.y > bounds.bottom
          ? this.pointer.y - bounds.bottom
          : 0;
    if (delta === 0) return;
    scroller.scrollTop += Math.max(-24, Math.min(24, delta * 0.2));
    this.onScroll(scroller.scrollTop);
    this.update();
    this.raf = requestAnimationFrame(this.tick);
  };

  private update() {
    const scroller = this.getScroller();
    if (!this.from || !scroller) return;
    const bounds = scroller.getBoundingClientRect();
    const x = Math.max(0, Math.min(scroller.clientWidth, this.pointer.x - bounds.left));
    const y = this.pointer.y - bounds.top + scroller.scrollTop;
    if (!this.rect && Math.hypot(x - this.from.x, y - this.from.y) < 4) return;
    this.rect = {
      x0: Math.min(this.from.x, x),
      y0: Math.min(this.from.y, y),
      x1: Math.max(this.from.x, x),
      y1: Math.max(this.from.y, y),
    };
    const pane = this.getPane();
    const from = Math.max(0, Math.floor(this.rect.y0 / FILE_ROW_HEIGHT));
    const to = Math.min(pane.visible.length - 1, Math.floor(this.rect.y1 / FILE_ROW_HEIGHT));
    const selected = new Set(this.base ?? []);
    for (let index = from; index <= to; index++) selected.add(pane.visible[index].name);
    pane.selected = selected;
  }

  private readonly onUp = () => {
    window.removeEventListener("pointermove", this.onMove);
    window.removeEventListener("pointerup", this.onUp);
    if (this.raf) cancelAnimationFrame(this.raf);
    this.raf = 0;
    if (this.rect) {
      this.justEnded = true;
      setTimeout(() => (this.justEnded = false), 0);
    } else if (this.base === null) {
      const pane = this.getPane();
      pane.selected = new Set();
      pane.anchor = null;
    }
    this.rect = null;
    this.from = null;
    this.base = null;
  };
}
