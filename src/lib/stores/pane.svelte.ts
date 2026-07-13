// Controller for one file panel (local or remote side of the dual-pane
// view, SPEC §5.2). Owns path, listing, sorting, filtering and selection.

import { commands, errorMessage, unwrap } from "$lib/api";
import type { RemoteEntry, S3AclStatus } from "$lib/api";
import { joinPath, parentPath } from "$lib/format";
import { isMod } from "$lib/platform";

export type PaneSide = "local" | "remote";
export type SortKey = "name" | "size" | "mtime" | "permissions";

/** GetObjectAcl batch size per backend call (SPEC §4.4 background fetch). */
const ACL_BATCH = 64;

export class PaneController {
  side: PaneSide;
  sessionId: string | null;
  /** Remote pane of an S3 session: ACL badges + public/private actions. */
  s3: boolean;

  path = $state("");
  entries = $state<RemoteEntry[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  filter = $state("");
  showHidden = $state(false);
  sortKey = $state<SortKey>("name");
  sortAsc = $state(true);
  /** Selected entry names (multi-select). */
  selected = $state(new Set<string>());
  anchor = $state<string | null>(null);
  /** Lazily loaded public/private badge per entry path (S3 only). */
  acl = $state<Record<string, S3AclStatus>>({});
  /** Invalidates in-flight ACL fetches when the listing changes. */
  private aclGeneration = 0;

  constructor(side: PaneSide, sessionId: string | null, showHidden: boolean, s3 = false) {
    this.side = side;
    this.sessionId = sessionId;
    this.showHidden = showHidden;
    this.s3 = s3;
  }

  readonly visible = $derived.by(() => {
    const q = this.filter.trim().toLowerCase();
    let list = this.entries.filter((e) => this.showHidden || !e.name.startsWith("."));
    if (q) list = list.filter((e) => e.name.toLowerCase().includes(q));
    const dir = this.sortAsc ? 1 : -1;
    const key = this.sortKey;
    return [...list].sort((a, b) => {
      if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1; // dirs first, always
      let cmp: number;
      switch (key) {
        case "size":
          cmp = a.size - b.size;
          break;
        case "mtime":
          cmp = (a.mtime ?? 0) - (b.mtime ?? 0);
          break;
        case "permissions":
          cmp = (a.permissions ?? 0) - (b.permissions ?? 0);
          break;
        default:
          cmp = a.name.localeCompare(b.name, undefined, { numeric: true });
      }
      return cmp !== 0 ? cmp * dir : a.name.localeCompare(b.name);
    });
  });

  readonly selectedEntries = $derived(this.visible.filter((e) => this.selected.has(e.name)));

  private async list(path: string): Promise<RemoteEntry[]> {
    if (this.side === "local") {
      return unwrap(commands.localList(path));
    }
    return unwrap(commands.remoteList(this.sessionId!, path));
  }

  async init(startDir: string | null) {
    let start = startDir;
    if (!start) {
      start =
        this.side === "local"
          ? await unwrap(commands.localHome())
          : await unwrap(commands.remoteHome(this.sessionId!));
    }
    await this.navigate(start);
  }

  async navigate(path: string) {
    this.loading = true;
    this.error = null;
    try {
      const entries = await this.list(path);
      this.entries = entries;
      this.path = path;
      this.selected = new Set();
      this.anchor = null;
      void this.loadAcl();
    } catch (e) {
      this.error = errorMessage(e);
    } finally {
      this.loading = false;
    }
  }

  async refresh() {
    try {
      this.entries = await this.list(this.path);
      const names = new Set(this.entries.map((e) => e.name));
      this.selected = new Set([...this.selected].filter((n) => names.has(n)));
      this.error = null;
      void this.loadAcl();
    } catch (e) {
      this.error = errorMessage(e);
    }
  }

  /** Background fetch of public/private badges — the listing shows
   *  instantly, statuses trickle in batches (SPEC §4.4). */
  async loadAcl() {
    if (!this.s3 || !this.sessionId) return;
    const generation = ++this.aclGeneration;
    this.acl = {};
    const files = this.entries.filter((e) => !e.is_dir).map((e) => e.path);
    for (let i = 0; i < files.length; i += ACL_BATCH) {
      const batch = files.slice(i, i + ACL_BATCH);
      try {
        const statuses = await unwrap(commands.s3AclStatus(this.sessionId, batch));
        if (generation !== this.aclGeneration) return; // pane navigated away
        for (const item of statuses) this.acl[item.path] = item.status;
      } catch {
        return; // session dropped — badges stay unknown
      }
    }
  }

  async up() {
    const parent = parentPath(this.path);
    if (parent !== this.path) await this.navigate(parent);
  }

  async open(entry: RemoteEntry) {
    if (entry.is_dir) await this.navigate(entry.path);
  }

  // -- selection --

  click(entry: RemoteEntry, e: MouseEvent) {
    if (isMod(e)) {
      const next = new Set(this.selected);
      if (next.has(entry.name)) next.delete(entry.name);
      else next.add(entry.name);
      this.selected = next;
      this.anchor = entry.name;
    } else if (e.shiftKey && this.anchor) {
      const list = this.visible;
      const a = list.findIndex((x) => x.name === this.anchor);
      const b = list.findIndex((x) => x.name === entry.name);
      if (a !== -1 && b !== -1) {
        const [from, to] = a < b ? [a, b] : [b, a];
        this.selected = new Set(list.slice(from, to + 1).map((x) => x.name));
      }
    } else {
      this.selected = new Set([entry.name]);
      this.anchor = entry.name;
    }
  }

  selectAll() {
    this.selected = new Set(this.visible.map((e) => e.name));
  }

  // -- operations --

  async mkdir(name: string) {
    const path = joinPath(this.path, name);
    if (this.side === "local") await unwrap(commands.localMkdir(path));
    else await unwrap(commands.remoteMkdir(this.sessionId!, path));
    await this.refresh();
  }

  async createFile(name: string) {
    const path = joinPath(this.path, name);
    if (this.side === "local") await unwrap(commands.localCreateFile(path));
    else await unwrap(commands.remoteCreateFile(this.sessionId!, path));
    await this.refresh();
  }

  async rename(entry: RemoteEntry, newName: string) {
    const to = joinPath(this.path, newName);
    if (this.side === "local") await unwrap(commands.localRename(entry.path, to));
    else await unwrap(commands.remoteRename(this.sessionId!, entry.path, to));
    await this.refresh();
  }

  async deleteEntries(entries: RemoteEntry[]) {
    for (const entry of entries) {
      if (this.side === "local") {
        await unwrap(commands.localDelete(entry.path));
      } else {
        await unwrap(commands.remoteDelete(this.sessionId!, entry.path, entry.is_dir));
      }
    }
    await this.refresh();
  }

  async chmod(entry: RemoteEntry, mode: number) {
    if (this.side === "local") await unwrap(commands.localChmod(entry.path, mode));
    else await unwrap(commands.remoteChmod(this.sessionId!, entry.path, mode));
    await this.refresh();
  }

  sortBy(key: SortKey) {
    if (this.sortKey === key) this.sortAsc = !this.sortAsc;
    else {
      this.sortKey = key;
      this.sortAsc = true;
    }
  }
}
