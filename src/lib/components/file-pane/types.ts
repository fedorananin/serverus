import type { RemoteEntry } from "$lib/api";
import type { MenuItem } from "../ContextMenu.svelte";

export const FILE_ROW_HEIGHT = 24;

export interface FilePaneNotice {
  text: string;
  error: boolean;
}

export interface FilePaneMenu {
  x: number;
  y: number;
  items: MenuItem[];
}

export type FilePaneDialog =
  | { kind: "mkdir" }
  | { kind: "newfile" }
  | { kind: "rename"; entry: RemoteEntry }
  | { kind: "delete"; entries: RemoteEntry[] }
  | { kind: "chmod"; entry: RemoteEntry };
