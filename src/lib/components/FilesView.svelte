<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import type { RemoteEntry, S3UploadAcl } from "$lib/api";
  import { commands, errorMessage, unwrap } from "$lib/api";
  import { s3PublicUrl } from "$lib/format";
  import { useAppModel } from "$lib/app/model.svelte";
  import { compareDirectoryEntries } from "$lib/directory-comparison";
  import { queueActivity, queueSettled } from "$lib/transfer-settle";
  import { PaneController } from "$lib/stores/pane.svelte";
  import { isMod } from "$lib/platform";
  import { vault } from "$lib/stores/vault.svelte";
  import FilePane from "./FilePane.svelte";
  import DirectoryComparisonBar from "./DirectoryComparisonBar.svelte";
  import TransferQueue from "./TransferQueue.svelte";
  import UploadAclDialog from "./UploadAclDialog.svelte";

  import type { Tab } from "$lib/stores/tabs.svelte";

  interface Props {
    tab: Tab;
    sessionId: string;
  }

  let { tab, sessionId }: Props = $props();
  const transfers = useAppModel().transfers;

  let root = $state<HTMLDivElement>();

  const connection = $derived(vault.data?.connections[tab.connectionId] ?? null);
  const showHidden = vault.data?.settings.panels.show_hidden ?? false;
  const isS3 = vault.data?.connections[tab.connectionId]?.protocol === "s3";
  const isFtp = vault.data?.connections[tab.connectionId]?.protocol === "ftp";

  const local = new PaneController("local", null, showHidden);
  const remote = new PaneController("remote", sessionId, showHidden, isS3);

  let transferError = $state<string | null>(null);
  let comparisonActive = $state(false);
  let differencesOnly = $state(false);
  const emptyComparison = compareDirectoryEntries([], []);
  const comparison = $derived.by(() =>
    comparisonActive
      ? // S3's listed mtime is server-managed upload time and can't be
        // preserved by transfers — comparing it would flag every uploaded
        // file as different forever. FTP's LIST mtime is real but coarse
        // (minutes, or date-only past ~6 months).
        compareDirectoryEntries(local.entries, remote.entries, {
          ignoreMtime: isS3,
          coarseRemoteMtime: isFtp,
        })
      : emptyComparison,
  );

  $effect(() => {
    local.comparisonStatuses = comparisonActive ? comparison.localStatuses : null;
    remote.comparisonStatuses = comparisonActive ? comparison.remoteStatuses : null;
    local.comparisonDifferencesOnly = comparisonActive && differencesOnly;
    remote.comparisonDifferencesOnly = comparisonActive && differencesOnly;
  });

  // Refresh both panes when this session's transfer queue settles — the
  // destination listing (and the comparison built from it) stays stale until
  // relisted.
  let previousActivity = queueActivity(transfers.summaryFor(sessionId));
  $effect(() => {
    const activity = queueActivity(transfers.summaryFor(sessionId));
    const settled = queueSettled(previousActivity, activity);
    previousActivity = activity;
    if (settled) {
      void local.refresh();
      void remote.refresh();
    }
  });

  function toggleComparison() {
    comparisonActive = !comparisonActive;
    if (!comparisonActive) differencesOnly = false;
  }

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

  /** Batch upload with a single upload-ACL prompt in "ask" mode. The whole
   *  selection goes down in one call so it shares one conflict batch and
   *  "apply to all remaining conflicts" covers every file. */
  async function uploadPaths(paths: string[]) {
    if (paths.length === 0 || !(await ensureUploadAcl(paths.length))) return;
    transferError = null;
    try {
      await transfers.upload(sessionId, paths, remote.path);
    } catch (e) {
      transferError = errorMessage(e);
    }
  }

  async function downloadPaths(paths: string[]) {
    if (paths.length === 0) return;
    transferError = null;
    try {
      await transfers.download(sessionId, paths, local.path);
    } catch (e) {
      transferError = errorMessage(e);
    }
  }

  function uploadSelection(entries: RemoteEntry[]) {
    void uploadPaths(entries.map((e) => e.path));
  }

  function downloadSelection(entries: RemoteEntry[]) {
    void downloadPaths(entries.map((e) => e.path));
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
    if (vault.screen !== "main") return;
    // Hidden tabs stay mounted and hear window keydowns too — only the
    // visible FilesView may act on the transfer shortcuts.
    if (!root || root.offsetParent === null) return;
    // Cmd/Ctrl+→ uploads the local selection, Cmd/Ctrl+← downloads the remote one.
    if (isMod(e) && e.key === "ArrowRight" && local.selected.size > 0) {
      e.preventDefault();
      uploadSelection(local.selectedEntries);
    } else if (isMod(e) && e.key === "ArrowLeft" && remote.selected.size > 0) {
      e.preventDefault();
      downloadSelection(remote.selectedEntries);
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="files-view" bind:this={root}>
  <DirectoryComparisonBar
    active={comparisonActive}
    summary={comparison.summary}
    {differencesOnly}
    ontoggle={toggleComparison}
    onfilterchange={(checked) => (differencesOnly = checked)}
  />
  <div class="files">
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
  <!-- Per-tab transfer panel: only this session's queue and history. -->
  <TransferQueue {sessionId} />
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
  .files-view {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
  }

  .files {
    display: flex;
    flex: 1;
    gap: 8px;
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
