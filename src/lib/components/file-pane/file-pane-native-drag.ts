import { startDrag } from "@crabnebula/tauri-plugin-drag";
import { commands, unwrap } from "$lib/api";

export class FilePaneNativeDrag {
  private armStart: { x: number; y: number } | null = null;
  private dragIconPromise: Promise<string> | null = null;

  constructor(private readonly selectedPaths: () => string[]) {}

  arm(event: PointerEvent) {
    this.armStart = { x: event.clientX, y: event.clientY };
    window.addEventListener("pointermove", this.onMove);
    window.addEventListener("pointerup", this.onUp);
  }

  disarm() {
    this.armStart = null;
    window.removeEventListener("pointermove", this.onMove);
    window.removeEventListener("pointerup", this.onUp);
  }

  private readonly onMove = (event: PointerEvent) => {
    if (!this.armStart) return;
    if (Math.hypot(event.clientX - this.armStart.x, event.clientY - this.armStart.y) < 6) return;
    const paths = this.selectedPaths();
    this.disarm();
    if (paths.length > 0) void this.launch(paths);
  };

  private readonly onUp = () => {
    this.disarm();
  };

  private dragIcon() {
    this.dragIconPromise ??= unwrap(commands.dragPreviewIcon());
    return this.dragIconPromise;
  }

  private async launch(paths: string[]) {
    try {
      await startDrag({ item: paths, icon: await this.dragIcon() });
    } catch {
      // Plugin unavailable (non-macOS / permission) — silently ignore.
    }
  }
}
