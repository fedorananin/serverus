<script lang="ts">
  import type { RemoteEntry, S3AclStatus, S3UploadAcl } from "$lib/api";
  import type { PaneController, SortKey } from "$lib/stores/pane.svelte";
  import { formatMtime, formatPermissions, formatSize } from "$lib/format";
  import { vault } from "$lib/stores/vault.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import InputDialog from "./InputDialog.svelte";
  import ChmodDialog from "./ChmodDialog.svelte";

  import { startDrag } from "@crabnebula/tauri-plugin-drag";
  import { commands, errorMessage, unwrap } from "$lib/api";
  import { dnd } from "$lib/stores/dnd.svelte";

  interface Props {
    pane: PaneController;
    title: string;
    /** Transfer the given entries towards the other pane. */
    ontransfer: (entries: RemoteEntry[]) => void;
    /** Remote double-click on a file (M5 remote edit). */
    onopenfile?: (entry: RemoteEntry) => void;
    /** Remote S3 pane: build an object's public URL (SPEC §4.4). */
    publicUrl?: (entry: RemoteEntry) => string | null;
    /** Remote S3 pane: ACL mode for uploads, shown as a header switch. */
    uploadMode?: S3UploadAcl | null;
    onuploadmode?: (mode: S3UploadAcl) => void;
  }

  let { pane, title, ontransfer, onopenfile, publicUrl, uploadMode, onuploadmode }: Props =
    $props();

  // -- S3 public/private actions (SPEC §4.4) --

  let aclNote = $state<{ text: string; error: boolean } | null>(null);
  let aclNoteTimer: ReturnType<typeof setTimeout> | undefined;
  function note(text: string, error = false) {
    aclNote = { text, error };
    clearTimeout(aclNoteTimer);
    aclNoteTimer = setTimeout(() => (aclNote = null), 6000);
  }

  async function setAcl(entries: RemoteEntry[], makePublic: boolean) {
    note(makePublic ? "Making public…" : "Making private…");
    try {
      const count = await unwrap(
        commands.s3SetAcl(
          pane.sessionId!,
          entries.map((e) => ({ path: e.path, is_dir: e.is_dir })),
          makePublic,
        ),
      );
      note(`${count} object${count === 1 ? "" : "s"} made ${makePublic ? "public" : "private"}`);
      void pane.loadAcl();
    } catch (e) {
      note(errorMessage(e), true);
    }
  }

  /** Run a pane operation, surfacing failures in the statusbar (they used
   *  to vanish as unhandled rejections). */
  function run(op: Promise<void>) {
    op.catch((e) => note(errorMessage(e), true));
  }

  function formatAcl(status: S3AclStatus | undefined): string {
    switch (status) {
      case "public":
        return "public";
      case "private":
        return "private";
      case "unknown":
        return "?";
      default:
        return "…";
    }
  }

  // Cached drag-cursor image path (the OS drag needs an on-disk icon).
  let dragIconPromise: Promise<string> | null = null;
  function dragIcon() {
    dragIconPromise ??= unwrap(commands.dragPreviewIcon());
    return dragIconPromise;
  }

  const ROW_H = 24;
  let scroller: HTMLDivElement | undefined = $state();
  let scrollTop = $state(0);
  let viewportH = $state(400);
  let pathInput = $state("");
  let editingPath = $state(false);

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let dialog = $state<
    | { kind: "mkdir" }
    | { kind: "newfile" }
    | { kind: "rename"; entry: RemoteEntry }
    | { kind: "delete"; entries: RemoteEntry[] }
    | { kind: "chmod"; entry: RemoteEntry }
    | null
  >(null);

  const sizeFormat = $derived(vault.data?.settings.panels.size_format ?? "kib");

  // Virtual scrolling window (SPEC §5.2: 10k+ files without lag).
  const total = $derived(pane.visible.length);
  const first = $derived(Math.max(0, Math.floor(scrollTop / ROW_H) - 10));
  const last = $derived(Math.min(total, Math.ceil((scrollTop + viewportH) / ROW_H) + 10));
  const window_ = $derived(pane.visible.slice(first, last));

  function startPathEdit() {
    pathInput = pane.path;
    editingPath = true;
  }

  function commitPath(e: Event) {
    e.preventDefault();
    editingPath = false;
    const target = pathInput.trim();
    if (target && target !== pane.path) void pane.navigate(target);
  }

  function rowDoubleClick(entry: RemoteEntry) {
    if (entry.is_dir) {
      void pane.open(entry);
    } else if (pane.side === "remote") {
      onopenfile?.(entry);
    }
  }

  function copyPath(entry: RemoteEntry) {
    void navigator.clipboard.writeText(entry.path);
  }

  function openMenu(e: MouseEvent, entry: RemoteEntry | null) {
    e.preventDefault();
    e.stopPropagation();
    if (entry && !pane.selected.has(entry.name)) {
      pane.selected = new Set([entry.name]);
      pane.anchor = entry.name;
    }
    const selection = entry ? pane.selectedEntries : [];
    const single = selection.length === 1 ? selection[0] : null;
    const items: MenuItem[] = [];
    if (entry) {
      items.push({
        label: pane.side === "local" ? "Upload →" : "← Download",
        action: () => ontransfer(selection),
      });
      if (single && !single.is_dir && pane.side === "remote" && onopenfile) {
        items.push({ label: "Edit…", action: () => onopenfile(single) });
      }
      items.push({ separator: true, label: "" });
      if (single) {
        items.push({ label: "Rename…", action: () => (dialog = { kind: "rename", entry: single }) });
        if (!pane.s3) {
          items.push({ label: "Permissions…", action: () => (dialog = { kind: "chmod", entry: single }) });
        }
      }
      if (pane.s3) {
        // S3: public/private instead of chmod; directories apply recursively.
        const suffix = selection.length > 1 ? ` (${selection.length})` : "";
        items.push({ label: `Make public${suffix}`, action: () => void setAcl(selection, true) });
        items.push({ label: `Make private${suffix}`, action: () => void setAcl(selection, false) });
        if (single && !single.is_dir && publicUrl) {
          const url = publicUrl(single);
          if (url) {
            items.push({
              label: "Copy public URL",
              action: () => void navigator.clipboard.writeText(url),
            });
          }
        }
      }
      if (single) {
        items.push({ label: "Copy path", action: () => copyPath(single) });
      }
      items.push({
        label: `Delete${selection.length > 1 ? ` (${selection.length})` : ""}`,
        danger: true,
        action: () => (dialog = { kind: "delete", entries: selection }),
      });
      items.push({ separator: true, label: "" });
    }
    items.push({ label: "New folder…", action: () => (dialog = { kind: "mkdir" }) });
    items.push({ label: "New file…", action: () => (dialog = { kind: "newfile" }) });
    items.push({ label: "Refresh", action: () => void pane.refresh() });
    menu = { x: e.clientX, y: e.clientY, items };
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === "F2" || e.key === "Enter") {
      const sel = pane.selectedEntries;
      if (sel.length === 1) {
        e.preventDefault();
        dialog = { kind: "rename", entry: sel[0] };
      }
    } else if (e.key === "Backspace" && !e.metaKey) {
      e.preventDefault();
      void pane.up();
    } else if (e.key === "a" && e.metaKey) {
      e.preventDefault();
      pane.selectAll();
    } else if (e.key === "Delete" || (e.key === "Backspace" && e.metaKey)) {
      const sel = pane.selectedEntries;
      if (sel.length > 0) {
        e.preventDefault();
        dialog = { kind: "delete", entries: sel };
      }
    }
  }

  // Two drag mechanisms, one per side (HTML5 DnD is dead in the Tauri webview):
  //  - Local pane → native OS drag (startDrag). Real file paths, so it can go
  //    OUT to Finder; dropped back on the remote pane it arrives as a Tauri
  //    file-drop and is routed to an upload by FilesView.
  //  - Remote pane → pointer drag with a ghost, dropped on the local pane to
  //    download. (Remote files aren't on disk, so no native/Finder drag.)
  function rowPointerDown(e: PointerEvent, entry: RemoteEntry) {
    if (e.button !== 0) return;
    // Modifier clicks are selection gestures — leave them to the click
    // handler (Cmd = toggle, Shift = range) and never start a drag.
    if (e.metaKey || e.shiftKey) return;
    // A press right of the filename text bubbles up to the rows container
    // and starts a marquee selection instead of a file drag.
    if (isRowWhitespace(e)) return;
    // Text selection is disabled via CSS (user-select:none) — no
    // preventDefault here, which would suppress click/dblclick.
    if (!pane.selected.has(entry.name)) {
      pane.selected = new Set([entry.name]);
      pane.anchor = entry.name;
    }
    if (pane.side === "local") {
      armNativeDrag(e);
    } else {
      const entries = pane.selectedEntries;
      const label =
        entries.length === 1 ? entries[0].name : `${entries.length} items`;
      dnd.begin(e, { kind: "files", side: "remote" }, label, () => {
        if (dnd.paneTarget === "local") ontransfer(entries);
      });
    }
  }

  // Native drag arms on pointerdown and fires once the pointer moves, so a
  // plain click still just selects.
  let armStart: { x: number; y: number } | null = null;
  function armNativeDrag(e: PointerEvent) {
    armStart = { x: e.clientX, y: e.clientY };
    window.addEventListener("pointermove", onArmMove);
    window.addEventListener("pointerup", onArmUp);
  }
  function onArmMove(e: PointerEvent) {
    if (!armStart) return;
    if (Math.hypot(e.clientX - armStart.x, e.clientY - armStart.y) < 6) return;
    const paths = pane.selectedEntries.map((x) => x.path);
    disarm();
    if (paths.length > 0) void launchNativeDrag(paths);
  }
  function onArmUp() {
    disarm();
  }
  function disarm() {
    armStart = null;
    window.removeEventListener("pointermove", onArmMove);
    window.removeEventListener("pointerup", onArmUp);
  }
  async function launchNativeDrag(paths: string[]) {
    try {
      await startDrag({ item: paths, icon: await dragIcon() });
    } catch {
      // Plugin unavailable (non-macOS / permission) — silently ignore.
    }
  }

  // -- marquee (rubber-band) selection --
  // Explorer-style: a drag starting on empty space, or on a row to the right
  // of its filename text, draws a selection rectangle. Coordinates are in
  // content space (y includes scrollTop) so the rectangle and the selection
  // survive virtual scrolling.
  let marquee = $state<{ x0: number; y0: number; x1: number; y1: number } | null>(null);
  let marqueeFrom: { x: number; y: number } | null = null;
  /** Selection to extend when the marquee started with Cmd/Shift held. */
  let marqueeBase: Set<string> | null = null;
  let marqueePointer = { x: 0, y: 0 };
  let marqueeRaf = 0;
  let marqueeJustEnded = false;

  /** True when the press is not on a row's icon/name text (Explorer-style
   *  hit test: only the filename is "the item", the rest is background). */
  function isRowWhitespace(e: PointerEvent): boolean {
    const row = (e.target as HTMLElement).closest?.(".row");
    const name = row?.querySelector(".cell.name");
    if (!name) return true;
    const range = document.createRange();
    range.selectNodeContents(name);
    return e.clientX > range.getBoundingClientRect().right + 6;
  }

  function rowsPointerDown(e: PointerEvent) {
    if (e.button !== 0 || !scroller) return;
    // Keep error text selectable (it has user-select: text for copying).
    if ((e.target as HTMLElement).closest?.(".pane-error")) return;
    const rect = scroller.getBoundingClientRect();
    // Ignore presses on a classic (non-overlay) vertical scrollbar.
    if (e.clientX - rect.left > scroller.clientWidth) return;
    if (!isRowWhitespace(e)) return; // the row itself starts a file drag
    marqueeFrom = { x: e.clientX - rect.left, y: e.clientY - rect.top + scroller.scrollTop };
    marqueeBase = e.metaKey || e.shiftKey ? new Set(pane.selected) : null;
    marqueePointer = { x: e.clientX, y: e.clientY };
    scroller.setPointerCapture(e.pointerId);
    window.addEventListener("pointermove", onMarqueeMove);
    window.addEventListener("pointerup", onMarqueeUp);
  }

  function onMarqueeMove(e: PointerEvent) {
    marqueePointer = { x: e.clientX, y: e.clientY };
    updateMarquee();
    if (!marqueeRaf) marqueeRaf = requestAnimationFrame(marqueeTick);
  }

  // Auto-scroll while the pointer is above/below the list; keeps running
  // between pointermove events so the selection grows without wiggling.
  function marqueeTick() {
    marqueeRaf = 0;
    if (!marqueeFrom || !scroller) return;
    const rect = scroller.getBoundingClientRect();
    const dy =
      marqueePointer.y < rect.top
        ? marqueePointer.y - rect.top
        : marqueePointer.y > rect.bottom
          ? marqueePointer.y - rect.bottom
          : 0;
    if (dy === 0) return;
    scroller.scrollTop += Math.max(-24, Math.min(24, dy * 0.2));
    scrollTop = scroller.scrollTop;
    updateMarquee();
    marqueeRaf = requestAnimationFrame(marqueeTick);
  }

  function updateMarquee() {
    if (!marqueeFrom || !scroller) return;
    const rect = scroller.getBoundingClientRect();
    const x = Math.max(0, Math.min(scroller.clientWidth, marqueePointer.x - rect.left));
    const y = marqueePointer.y - rect.top + scroller.scrollTop;
    if (!marquee && Math.hypot(x - marqueeFrom.x, y - marqueeFrom.y) < 4) return;
    marquee = {
      x0: Math.min(marqueeFrom.x, x),
      y0: Math.min(marqueeFrom.y, y),
      x1: Math.max(marqueeFrom.x, x),
      y1: Math.max(marqueeFrom.y, y),
    };
    const list = pane.visible;
    const from = Math.max(0, Math.floor(marquee.y0 / ROW_H));
    const to = Math.min(list.length - 1, Math.floor(marquee.y1 / ROW_H));
    const next = new Set(marqueeBase ?? []);
    for (let i = from; i <= to; i++) next.add(list[i].name);
    pane.selected = next;
  }

  function onMarqueeUp() {
    window.removeEventListener("pointermove", onMarqueeMove);
    window.removeEventListener("pointerup", onMarqueeUp);
    if (marqueeRaf) cancelAnimationFrame(marqueeRaf);
    marqueeRaf = 0;
    if (marquee) {
      // Swallow the click that follows the drag so a row under the pointer
      // can't collapse the fresh selection.
      marqueeJustEnded = true;
      setTimeout(() => (marqueeJustEnded = false), 0);
    } else if (marqueeBase === null) {
      // Plain click on background clears the selection.
      pane.selected = new Set();
      pane.anchor = null;
    }
    marquee = null;
    marqueeFrom = null;
    marqueeBase = null;
  }

  const paneDropActive = $derived(
    dnd.active?.kind === "files" && pane.side === "local" && dnd.paneTarget === "local",
  );

  async function applyChmod(entry: RemoteEntry, mode: number, recursive: "files" | "dirs" | "both" | null) {
    if (!recursive) {
      await pane.chmod(entry, mode);
      return;
    }
    // Recursive chmod: walk from the UI via listings (depth-first).
    const stack = [entry.path];
    const applyDirs = recursive !== "files";
    const applyFiles = recursive !== "dirs";
    if (applyDirs) await pane.chmod(entry, mode);
    const { commands, unwrap } = await import("$lib/api");
    while (stack.length) {
      const dir = stack.pop()!;
      const children =
        pane.side === "local"
          ? await unwrap(commands.localList(dir))
          : await unwrap(commands.remoteList(pane.sessionId!, dir));
      for (const child of children) {
        if (child.is_dir && !child.is_symlink) {
          stack.push(child.path);
          if (applyDirs) {
            if (pane.side === "local") await unwrap(commands.localChmod(child.path, mode));
            else await unwrap(commands.remoteChmod(pane.sessionId!, child.path, mode));
          }
        } else if (applyFiles) {
          if (pane.side === "local") await unwrap(commands.localChmod(child.path, mode));
          else await unwrap(commands.remoteChmod(pane.sessionId!, child.path, mode));
        }
      }
    }
    await pane.refresh();
  }

  function sortIndicator(key: SortKey) {
    return pane.sortKey === key ? (pane.sortAsc ? " ↑" : " ↓") : "";
  }
</script>

<div
  class="pane"
  class:drag-over={paneDropActive}
  data-pane={pane.side}
  role="region"
  aria-label={title}
  onpointermove={() => {
    // Only the local pane is a pointer-drag drop target (remote→local
    // download); the remote pane receives Finder/native drops via Tauri.
    if (dnd.active?.kind === "files" && pane.side === "local") dnd.paneTarget = "local";
  }}
  onpointerleave={() => {
    if (dnd.paneTarget === pane.side) dnd.paneTarget = null;
  }}
>
  <div class="pane-head">
    <span class="pane-title">{title}</span>
    {#if uploadMode && onuploadmode}
      <select
        class="acl-mode"
        title="Access for uploaded files"
        value={uploadMode}
        onchange={(e) => onuploadmode(e.currentTarget.value as S3UploadAcl)}
      >
        <option value="private">upload: private</option>
        <option value="public_read">upload: public</option>
        <option value="ask">upload: ask</option>
      </select>
    {/if}
    <button class="tool" title="Up" aria-label="Up" onclick={() => void pane.up()}>↑</button>
    <button class="tool" title="Refresh" aria-label="Refresh" onclick={() => void pane.refresh()}>⟳</button>
    <button
      class="tool"
      title="Toggle hidden files"
      aria-label="Toggle hidden files"
      class:on={pane.showHidden}
      onclick={() => (pane.showHidden = !pane.showHidden)}>.*</button
    >
  </div>

  <div class="pathbar">
    {#if editingPath}
      <form onsubmit={commitPath} class="path-form">
        <input
          type="text"
          class="mono path-input"
          bind:value={pathInput}
          onblur={() => (editingPath = false)}
          use:autofocus
        />
      </form>
    {:else}
      <button class="path mono" title={pane.path} onclick={startPathEdit}>{pane.path}</button>
    {/if}
    <input type="text" class="filter" placeholder="Filter" bind:value={pane.filter} />
  </div>

  <div class="cols">
    <button class="col name" onclick={() => pane.sortBy("name")}>Name{sortIndicator("name")}</button>
    <button class="col size" onclick={() => pane.sortBy("size")}>Size{sortIndicator("size")}</button>
    <button class="col date" onclick={() => pane.sortBy("mtime")}>Date{sortIndicator("mtime")}</button>
    <button class="col perm" onclick={() => pane.sortBy("permissions")}
      >{pane.s3 ? "Access" : "Mode"}{sortIndicator("permissions")}</button
    >
  </div>

  <div
    class="rows"
    bind:this={scroller}
    bind:clientHeight={viewportH}
    onscroll={() => (scrollTop = scroller?.scrollTop ?? 0)}
    role="listbox"
    aria-label="{title} files"
    tabindex="0"
    onkeydown={keydown}
    onpointerdown={rowsPointerDown}
    oncontextmenu={(e) => openMenu(e, null)}
  >
    {#if pane.error}
      <div class="pane-error">{pane.error}</div>
    {:else if pane.loading}
      <div class="pane-empty">Loading…</div>
    {:else if total === 0}
      <div class="pane-empty">Empty</div>
    {:else}
      <div style:height="{total * ROW_H}px" class="spacer">
        <div style:transform="translateY({first * ROW_H}px)">
          {#each window_ as entry (entry.path)}
            <div
              class="row mono"
              class:selected={pane.selected.has(entry.name)}
              role="option"
              aria-selected={pane.selected.has(entry.name)}
              tabindex="-1"
              onclick={(e) => !marqueeJustEnded && pane.click(entry, e)}
              ondblclick={() => rowDoubleClick(entry)}
              oncontextmenu={(e) => openMenu(e, entry)}
              onpointerdown={(e) => rowPointerDown(e, entry)}
              onkeydown={() => {}}
            >
              <span class="cell name">
                <span class="icon">{entry.is_dir ? "📁" : "📄"}</span>
                {entry.name}{entry.is_symlink ? " →" : ""}
              </span>
              <span class="cell size">{entry.is_dir ? "—" : formatSize(entry.size, sizeFormat)}</span>
              <span class="cell date">{formatMtime(entry.mtime)}</span>
              {#if pane.s3}
                <span class="cell perm" class:acl-public={pane.acl[entry.path] === "public"}>
                  {entry.is_dir ? "" : formatAcl(pane.acl[entry.path])}
                </span>
              {:else}
                <span class="cell perm">{formatPermissions(entry.permissions)}</span>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}
    {#if marquee}
      <div
        class="marquee"
        style:left="{marquee.x0}px"
        style:top="{marquee.y0}px"
        style:width="{marquee.x1 - marquee.x0}px"
        style:height="{marquee.y1 - marquee.y0}px"
      ></div>
    {/if}
  </div>

  <div class="statusbar">
    <span>{total} items{pane.selected.size > 0 ? `, ${pane.selected.size} selected` : ""}</span>
    {#if aclNote}
      <span class="acl-note" class:err={aclNote.error}>{aclNote.text}</span>
    {/if}
  </div>
</div>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

{#if dialog?.kind === "mkdir"}
  <InputDialog
    title="New folder"
    placeholder="folder name"
    confirmLabel="Create"
    onsubmit={(name) => run(pane.mkdir(name))}
    onclose={() => (dialog = null)}
  />
{:else if dialog?.kind === "newfile"}
  <InputDialog
    title="New file"
    placeholder="file name"
    confirmLabel="Create"
    onsubmit={(name) => run(pane.createFile(name))}
    onclose={() => (dialog = null)}
  />
{:else if dialog?.kind === "rename"}
  <InputDialog
    title="Rename"
    initial={dialog.entry.name}
    confirmLabel="Rename"
    onsubmit={(name) => dialog?.kind === "rename" && run(pane.rename(dialog.entry, name))}
    onclose={() => (dialog = null)}
  />
{:else if dialog?.kind === "delete"}
  <ConfirmDialog
    title="Delete"
    message={dialog.entries.length === 1
      ? `Delete "${dialog.entries[0].name}"${dialog.entries[0].is_dir ? " and all its contents" : ""}?`
      : `Delete ${dialog.entries.length} items (folders recursively)?`}
    onconfirm={() => dialog?.kind === "delete" && run(pane.deleteEntries(dialog.entries))}
    onclose={() => (dialog = null)}
  />
{:else if dialog?.kind === "chmod"}
  <ChmodDialog
    entry={dialog.entry}
    onapply={(mode, recursive) =>
      dialog?.kind === "chmod" && run(applyChmod(dialog.entry, mode, recursive))}
    onclose={() => (dialog = null)}
  />
{/if}

<script lang="ts" module>
  function autofocus(node: HTMLInputElement) {
    node.focus();
    node.select();
  }
</script>

<style>
  .pane {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-1);
    overflow: hidden;
  }

  .pane.drag-over {
    border-color: var(--accent);
  }

  .pane-head {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 5px 8px 0;
  }

  .pane-title {
    flex: 1;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-1);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .tool {
    padding: 1px 7px;
    font-size: 12px;
    background: transparent;
    border-color: transparent;
    color: var(--text-1);
  }

  .tool:hover {
    background: var(--bg-3);
  }

  .tool.on {
    color: var(--accent);
  }

  .acl-mode {
    font-size: 10px;
    padding: 1px 4px;
  }

  .pathbar {
    display: flex;
    gap: 6px;
    padding: 5px 8px;
  }

  .path,
  .path-input {
    flex: 1;
    min-width: 0;
    text-align: left;
    font-size: 11px;
    padding: 3px 8px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    direction: rtl;
  }

  .path-form {
    flex: 1;
    display: flex;
  }

  .path-input {
    direction: ltr;
    width: 100%;
  }

  .filter {
    width: 84px;
    font-size: 11px;
    padding: 3px 7px;
  }

  .cols {
    display: flex;
    border-bottom: 1px solid var(--border);
    padding: 0 8px;
  }

  .col {
    background: transparent;
    border: none;
    color: var(--text-2);
    font-size: 10px;
    text-align: left;
    padding: 3px 4px;
  }

  .col.name {
    flex: 1;
  }

  .col.size {
    width: 66px;
  }

  .col.date {
    width: 78px;
  }

  .col.perm {
    width: 80px;
  }

  .rows {
    flex: 1;
    overflow-y: auto;
    outline: none;
    position: relative;
    user-select: none;
    -webkit-user-select: none;
  }

  .spacer {
    position: relative;
    overflow: hidden;
  }

  .row {
    display: flex;
    height: 24px;
    align-items: center;
    padding: 0 8px;
    font-size: 11.5px;
    cursor: default;
    user-select: none;
    -webkit-user-select: none;
  }

  .row:hover {
    background: var(--bg-2);
  }

  .row.selected {
    background: var(--accent-subtle);
  }

  .marquee {
    position: absolute;
    z-index: 5;
    border: 1px solid var(--accent);
    background: var(--accent-subtle);
    opacity: 0.6;
    pointer-events: none;
  }

  .cell {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cell.name {
    flex: 1;
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .icon {
    font-size: 11px;
  }

  .cell.size {
    width: 66px;
    text-align: right;
    color: var(--text-1);
  }

  .cell.date {
    width: 78px;
    padding-left: 8px;
    color: var(--text-1);
  }

  .cell.perm {
    width: 80px;
    padding-left: 8px;
    color: var(--text-2);
  }

  .cell.perm.acl-public {
    color: var(--warning);
  }

  .pane-empty,
  .pane-error {
    padding: 20px;
    text-align: center;
    color: var(--text-2);
    font-size: 12px;
  }

  .pane-error {
    color: var(--danger);
    user-select: text;
  }

  .statusbar {
    padding: 3px 10px;
    border-top: 1px solid var(--border);
    font-size: 10px;
    color: var(--text-2);
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }

  .acl-note {
    color: var(--text-1);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .acl-note.err {
    color: var(--danger);
  }
</style>
