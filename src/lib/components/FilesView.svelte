<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import type { RemoteEntry, S3UploadAcl } from "$lib/api";
  import { commands, errorMessage, unwrap } from "$lib/api";
  import { s3PublicUrl } from "$lib/format";
  import { PaneController } from "$lib/stores/pane.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import FilePane from "./FilePane.svelte";
  import UploadAclDialog from "./UploadAclDialog.svelte";

  import type { Tab } from "$lib/stores/tabs.svelte";

  interface Props {
    tab: Tab;
    sessionId: string;
  }

  let { tab, sessionId }: Props = $props();

  let root = $state<HTMLDivElement>();

  const connection = $derived(vault.data?.connections[tab.connectionId] ?? null);
  const showHidden = vault.data?.settings.panels.show_hidden ?? false;
  const isS3 = vault.data?.connections[tab.connectionId]?.protocol === "s3";

  const local = new PaneController("local", null, showHidden);
  const remote = new PaneController("remote", sessionId, showHidden, isS3);

  let transferError = $state<string | null>(null);

  // -- S3 upload ACL (SPEC §4.4): pane switch + the "ask" dialog --

  const uploadMode = $derived(isS3 ? (connection?.s3?.upload_acl ?? "private") : null);
  let askUpload = $state<{
    count: number;
    resolve: (choice: "private" | "public_read" | null) => void;
  } | null>(null);

  async function setUploadMode(mode: S3UploadAcl) {
    try {
      const updated = await unwrap(commands.s3SetUploadAcl(sessionId, mode, true));
      if (updated) vault.data = updated;
    } catch (e) {
      transferError = errorMessage(e);
    }
  }

  /** In "ask" mode, resolve the batch ACL before enqueueing; false = cancel. */
  async function ensureUploadAcl(count: number): Promise<boolean> {
    if (!isS3 || uploadMode !== "ask") return true;
    const choice = await new Promise<"private" | "public_read" | null>((resolve) => {
      askUpload = { count, resolve };
    });
    askUpload = null;
    if (!choice) return false;
    try {
      // Applies to this session only — the stored mode stays "ask".
      await unwrap(commands.s3SetUploadAcl(sessionId, choice, false));
      return true;
    } catch (e) {
      transferError = errorMessage(e);
      return false;
    }
  }

  // Remember the remote dir so a reconnect restores it (SPEC §4.1).
  $effect(() => {
    if (remote.path) tab.lastRemoteDir = remote.path;
  });

  onMount(() => {
    const localStart =
      connection?.local_dir ?? vault.data?.settings.panels.default_local_dir ?? null;
    void local.init(localStart || null);
    void remote.init(tab.lastRemoteDir || connection?.remote_dir || null);

    // OS file drops land here: from Finder, and from the local pane's own
    // native drag when released back inside the window. Route by which pane
    // sits under the cursor: remote → upload, local → local copy.
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (event.payload.type !== "drop") return;
      const paths = event.payload.paths;
      // wry on macOS reports drop positions in logical (CSS) coordinates
      // already — NSDraggingInfo.draggingLocation in points, wrapped
      // unscaled into a "PhysicalPosition". Dividing by devicePixelRatio
      // here halved every coordinate on Retina and routed drops to the
      // wrong pane. (Windows/Linux wry does emit physical pixels — a port
      // must scale per-platform.)
      const el = document.elementFromPoint(
        event.payload.position.x,
        event.payload.position.y,
      );
      // Every open tab keeps its FilesView mounted (SessionView only
      // toggles display), and each one hears this webview-wide event —
      // only the instance owning the pane under the cursor may act, or a
      // single drop would upload to every connected session.
      if (!el || !root?.contains(el)) return;
      const side = el.closest("[data-pane]")?.getAttribute("data-pane");
      if (side === "remote") {
        void uploadPaths(paths);
      } else if (side === "local") {
        void localCopy(paths);
      }
    });
    return () => void unlisten.then((f) => f());
  });

  async function localCopy(paths: string[]) {
    transferError = null;
    try {
      await unwrap(commands.localCopyInto(paths, local.path));
      await local.refresh();
    } catch (e) {
      transferError = errorMessage(e);
    }
  }

  async function upload(localPath: string) {
    transferError = null;
    try {
      await unwrap(commands.transferUpload(sessionId, localPath, remote.path));
      // Refresh once the queue settles is handled by the queue panel; do a
      // short delayed refresh for quick small files.
      setTimeout(() => void remote.refresh(), 800);
    } catch (e) {
      transferError = errorMessage(e);
    }
  }

  /** Batch upload with a single upload-ACL prompt in "ask" mode. */
  async function uploadPaths(paths: string[]) {
    if (paths.length === 0 || !(await ensureUploadAcl(paths.length))) return;
    for (const p of paths) void upload(p);
  }

  async function download(remotePath: string) {
    transferError = null;
    try {
      await unwrap(commands.transferDownload(sessionId, remotePath, local.path));
      setTimeout(() => void local.refresh(), 800);
    } catch (e) {
      transferError = errorMessage(e);
    }
  }

  function uploadSelection(entries: RemoteEntry[]) {
    void uploadPaths(entries.map((e) => e.path));
  }

  function downloadSelection(entries: RemoteEntry[]) {
    for (const entry of entries) void download(entry.path);
  }

  // Remote edit (SPEC §5.3): double-click downloads to the edit cache and
  // opens the editor; saves upload back automatically.
  async function openForEdit(entry: RemoteEntry) {
    transferError = null;
    try {
      await unwrap(commands.remoteEditOpen(sessionId, entry.path));
    } catch (e) {
      transferError = errorMessage(e);
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    // Hidden tabs stay mounted and hear window keydowns too — only the
    // visible FilesView may act on the transfer shortcuts.
    if (!root || root.offsetParent === null) return;
    // Cmd+→ uploads the local selection, Cmd+← downloads the remote one.
    if (e.metaKey && e.key === "ArrowRight" && local.selected.size > 0) {
      e.preventDefault();
      uploadSelection(local.selectedEntries);
    } else if (e.metaKey && e.key === "ArrowLeft" && remote.selected.size > 0) {
      e.preventDefault();
      downloadSelection(remote.selectedEntries);
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="files" bind:this={root}>
  <FilePane pane={local} title="Local" ontransfer={uploadSelection} />
  <FilePane
    pane={remote}
    title={connection?.name ?? "Remote"}
    ontransfer={downloadSelection}
    onopenfile={(entry) => void openForEdit(entry)}
    publicUrl={isS3 && connection ? (entry) => s3PublicUrl(connection, entry.path) : undefined}
    {uploadMode}
    onuploadmode={(mode) => void setUploadMode(mode)}
  />
</div>

{#if askUpload}
  <UploadAclDialog count={askUpload.count} onchoice={(choice) => askUpload?.resolve(choice)} />
{/if}

{#if transferError}
  <div class="transfer-error">
    {transferError}
    <button class="dismiss" onclick={() => (transferError = null)}>✕</button>
  </div>
{/if}

<style>
  .files {
    display: flex;
    gap: 8px;
    height: 100%;
    min-height: 0;
    padding: 8px;
  }

  .transfer-error {
    position: absolute;
    bottom: 12px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--danger-subtle);
    border: 1px solid var(--danger);
    border-radius: var(--radius);
    color: var(--text-0);
    padding: 6px 12px;
    font-size: 12px;
    display: flex;
    gap: 10px;
    align-items: center;
  }

  .dismiss {
    background: transparent;
    border: none;
    color: var(--text-1);
    padding: 0 2px;
  }
</style>
