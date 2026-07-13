// Pointer-based drag & drop. HTML5 DnD does not work inside Tauri's
// WKWebView (the native handler that powers Finder file drops intercepts
// it), so in-app dragging is implemented with pointer events + a ghost.

export type DragPayload =
  | { kind: "tree-node"; id: string }
  | { kind: "files"; side: "local" | "remote" };

export type TreeZone = "before" | "into" | "after";

interface Pending {
  payload: DragPayload;
  label: string;
  startX: number;
  startY: number;
  drop: () => void;
}

const DRAG_THRESHOLD = 5;

class DndStore {
  /** Set once the pointer moved past the threshold. */
  active = $state<DragPayload | null>(null);
  label = $state("");
  x = $state(0);
  y = $state(0);

  /** Sidebar tree target (row under the pointer + zone within it). */
  treeTarget = $state<{ id: string; zone: TreeZone } | null>(null);
  /** True while the pointer is over the sidebar tree background. */
  treeRootHover = $state(false);
  /** File pane currently under the pointer. */
  paneTarget = $state<"local" | "remote" | null>(null);

  private pending: Pending | null = null;

  /**
   * Arm a potential drag on pointerdown. The drag starts after the pointer
   * moves; a plain click never turns into a drag. `drop` runs on release
   * when a drag actually happened — it reads the current targets.
   */
  begin(e: PointerEvent, payload: DragPayload, label: string, drop: () => void) {
    if (e.button !== 0) return;
    this.pending = { payload, label, startX: e.clientX, startY: e.clientY, drop };
    window.addEventListener("pointermove", this.onMove);
    window.addEventListener("pointerup", this.onUp);
  }

  private onMove = (e: PointerEvent) => {
    const p = this.pending;
    if (!p) return;
    if (!this.active) {
      const dist = Math.hypot(e.clientX - p.startX, e.clientY - p.startY);
      if (dist < DRAG_THRESHOLD) return;
      this.active = p.payload;
      this.label = p.label;
    }
    this.x = e.clientX;
    this.y = e.clientY;
  };

  private onUp = () => {
    const p = this.pending;
    const dragged = this.active !== null;
    this.pending = null;
    window.removeEventListener("pointermove", this.onMove);
    window.removeEventListener("pointerup", this.onUp);
    if (dragged && p) p.drop();
    this.active = null;
    this.treeTarget = null;
    this.treeRootHover = false;
    this.paneTarget = null;
  };
}

export const dnd = new DndStore();
