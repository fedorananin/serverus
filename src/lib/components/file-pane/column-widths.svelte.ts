import type { ColumnWidths, Settings } from "$lib/api";
import { errorMessage } from "$lib/api";
import { vault } from "$lib/stores/vault.svelte";
import { showToast } from "../Toasts.svelte";

export type FileColumn = keyof ColumnWidths;

// Bounds and defaults mirror settings.rs (COLUMN_WIDTH_*, ColumnWidths): the
// backend clamps on write, so anything out of range here would be silently
// corrected.
const WIDTH_MIN = 40;
const WIDTH_MAX = 320;
export const DEFAULT_COLUMN_WIDTHS: ColumnWidths = { size: 74, date: 100, mode: 80 };

const clampWidth = (w: number) => Math.round(Math.min(WIDTH_MAX, Math.max(WIDTH_MIN, w)));

/** Drag-resizable Size / Date / Mode columns of one file pane. The fixed
 *  columns are pinned to the pane's right edge and Name takes the rest, so
 *  each handle sits on a column's left edge and the boundary follows the
 *  pointer. Widths are shared by every pane through the vault settings. */
export class FileColumnWidths {
  // Set only while dragging (and until the write lands), so the boundary
  // follows the pointer without a vault write per pixel.
  private drag = $state<ColumnWidths | null>(null);

  readonly stored = $derived<ColumnWidths>(
    vault.data?.settings.panels.columns ?? DEFAULT_COLUMN_WIDTHS,
  );
  readonly current = $derived<ColumnWidths>(this.drag ?? this.stored);

  /** CSS custom properties consumed by the header and the rows. */
  readonly style = $derived(
    `--col-size: ${this.current.size}px; --col-date: ${this.current.date}px; ` +
      `--col-mode: ${this.current.mode}px`,
  );

  startResize(column: FileColumn, event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const startX = event.clientX;
    const start = { ...this.current };
    const handle = event.currentTarget as HTMLElement;
    handle.setPointerCapture(event.pointerId);

    const onMove = (ev: PointerEvent) => {
      this.drag = { ...start, [column]: clampWidth(start[column] - (ev.clientX - startX)) };
    };
    // A drag also ends without a pointerup — ⌘Tab away, a system gesture —
    // and the column must not keep following a pointer with no button down.
    // Lost capture follows every pointerup too, hence the guard.
    let ended = false;
    const onEnd = async () => {
      if (ended) return;
      ended = true;
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onEnd);
      window.removeEventListener("pointercancel", onEnd);
      handle.removeEventListener("lostpointercapture", onEnd);
      const final = this.drag;
      if (final === null) return; // press without movement
      await this.persist(final);
      // Cleared only after the write lands, so the column never snaps back
      // to the old width for a frame.
      this.drag = null;
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onEnd);
    window.addEventListener("pointercancel", onEnd);
    handle.addEventListener("lostpointercapture", onEnd);
  }

  /** Double-click on a handle: back to the default width. */
  reset(column: FileColumn) {
    void this.persist({ ...this.stored, [column]: DEFAULT_COLUMN_WIDTHS[column] });
  }

  private async persist(widths: ColumnWidths) {
    const current = vault.data?.settings;
    const stored = this.stored;
    if (
      !current ||
      (widths.size === stored.size && widths.date === stored.date && widths.mode === stored.mode)
    ) {
      return;
    }
    const next = $state.snapshot(current) as Settings;
    next.panels.columns = widths;
    try {
      await vault.updateSettings(next);
    } catch (e) {
      // Only realistic cause is the vault locking mid-drag. The width reverts
      // on its own once the drag clears; say why rather than revert silently.
      showToast(`Could not save column width: ${errorMessage(e)}`, true);
    }
  }
}
