<script lang="ts">
  import type { RemoteEntry, S3UploadAcl } from "$lib/api";
  import { commands, errorMessage, unwrap } from "$lib/api";
  import { copyPublicUrl } from "$lib/public-url-clipboard";
  import { dnd } from "$lib/stores/dnd.svelte";
  import type { PaneController } from "$lib/stores/pane.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import type { MenuItem } from "./ContextMenu.svelte";
  import FilePaneOverlays from "./file-pane/FilePaneOverlays.svelte";
  import FilePaneStatus from "./file-pane/FilePaneStatus.svelte";
  import FilePaneToolbar from "./file-pane/FilePaneToolbar.svelte";
  import FilePaneViewport from "./file-pane/FilePaneViewport.svelte";
  import type { FilePaneDialog, FilePaneMenu, FilePaneNotice } from "./file-pane/types";

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
  let notice = $state<FilePaneNotice | null>(null);
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;
  let menu = $state<FilePaneMenu | null>(null);
  let dialog = $state<FilePaneDialog | null>(null);

  const sizeFormat = $derived(vault.data?.settings.panels.size_format ?? "kib");
  const paneDropActive = $derived(
    dnd.active?.kind === "files" && pane.side === "local" && dnd.paneTarget === "local",
  );

  function note(text: string, error = false) {
    notice = { text, error };
    clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => (notice = null), 6000);
  }

  async function setAcl(entries: RemoteEntry[], makePublic: boolean) {
    note(makePublic ? "Making public…" : "Making private…");
    try {
      const count = await unwrap(
        commands.s3SetAcl(
          pane.sessionId!,
          entries.map((entry) => ({ path: entry.path, is_dir: entry.is_dir })),
          makePublic,
        ),
      );
      note(`${count} object${count === 1 ? "" : "s"} made ${makePublic ? "public" : "private"}`);
      void pane.loadAcl();
    } catch (error) {
      note(errorMessage(error), true);
    }
  }

  function copyPath(entry: RemoteEntry) {
    void navigator.clipboard.writeText(entry.path);
  }

  function openMenu(event: MouseEvent | KeyboardEvent, entry: RemoteEntry | null) {
    event.preventDefault();
    event.stopPropagation();
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
      if (single?.is_dir) {
        items.push({ label: "Open", action: () => void pane.open(single) });
      }
      if (single && !single.is_dir && pane.side === "remote" && onopenfile) {
        items.push({ label: "Edit…", action: () => onopenfile(single) });
      }
      items.push({ separator: true, label: "" });
      if (single) {
        items.push({ label: "Rename…", action: () => (dialog = { kind: "rename", entry: single }) });
        if (!pane.s3 && !(pane.side === "local" && single.permissions == null)) {
          items.push({
            label: "Permissions…",
            action: () => (dialog = { kind: "chmod", entry: single }),
          });
        }
      }
      if (pane.s3) {
        const suffix = selection.length > 1 ? ` (${selection.length})` : "";
        items.push({ label: `Make public${suffix}`, action: () => void setAcl(selection, true) });
        items.push({ label: `Make private${suffix}`, action: () => void setAcl(selection, false) });
        if (single && !single.is_dir && publicUrl) {
          const url = publicUrl(single);
          if (url) {
            items.push({
              label: "Copy public URL",
              action: () =>
                void copyPublicUrl(url).then((result) => note(result.text, result.error)),
            });
          }
        }
      }
      if (single) items.push({ label: "Copy path", action: () => copyPath(single) });
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
    const x = event instanceof MouseEvent ? event.clientX : 16;
    const y = event instanceof MouseEvent ? event.clientY : 16;
    menu = { x, y, items };
  }
</script>

<div
  class="pane"
  class:drag-over={paneDropActive}
  data-pane={pane.side}
  role="region"
  aria-label={title}
  onpointermove={() => {
    if (dnd.active?.kind === "files" && pane.side === "local") dnd.paneTarget = "local";
  }}
  onpointerleave={() => {
    if (dnd.paneTarget === pane.side) dnd.paneTarget = null;
  }}
>
  <FilePaneToolbar
    {pane}
    {title}
    {uploadMode}
    {onuploadmode}
    onactions={(event) => openMenu(event, pane.selectedEntries[0] ?? null)}
  />
  <FilePaneViewport
    {pane}
    {title}
    {sizeFormat}
    {ontransfer}
    {onopenfile}
    onmenu={openMenu}
    ondialog={(value) => (dialog = value)}
  />
  <FilePaneStatus
    total={pane.visible.length}
    selectedCount={pane.selected.size}
    {notice}
  />
</div>

<FilePaneOverlays
  {pane}
  {menu}
  {dialog}
  onclosemenu={() => (menu = null)}
  onclosedialog={() => (dialog = null)}
  onerror={(message) => note(message, true)}
/>

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
</style>
