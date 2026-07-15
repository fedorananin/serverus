<script lang="ts">
  import type { Badge, PublicConnection, Settings, TreeNode } from "$lib/api";
  import { errorMessage } from "$lib/api";
  import { vault } from "$lib/stores/vault.svelte";
  import { tabs } from "$lib/stores/tabs.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import ConnectionDialog from "./ConnectionDialog.svelte";
  import FolderDialog from "./FolderDialog.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import SidebarTree from "./SidebarTree.svelte";
  import { showToast } from "./Toasts.svelte";

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let connectionDialog = $state<{ existing: PublicConnection | null; parent: string | null } | null>(null);
  let folderDialog = $state<{
    existing: { id: string; name: string; badge: Badge | null } | null;
    parent: string | null;
  } | null>(null);
  let confirm = $state<{ title: string; message: string; action: () => void } | null>(null);

  // Sidebar resize. Bounds mirror model.rs (SIDEBAR_WIDTH_*): the backend
  // clamps on write, so anything out of range here would be silently corrected.
  const WIDTH_MIN = 200;
  const WIDTH_MAX = 380;
  const WIDTH_DEFAULT = 230;

  const clampWidth = (w: number) => Math.round(Math.min(WIDTH_MAX, Math.max(WIDTH_MIN, w)));

  // Set only while dragging (and until the write lands), so the edge follows
  // the pointer without a vault write per pixel.
  let dragWidth = $state<number | null>(null);
  const storedWidth = $derived(clampWidth(vault.data?.settings.panels.sidebar_width ?? WIDTH_DEFAULT));
  const width = $derived(dragWidth ?? storedWidth);

  async function persistWidth(w: number) {
    const current = vault.data?.settings;
    if (!current || w === storedWidth) return;
    const next = $state.snapshot(current) as Settings;
    next.panels.sidebar_width = w;
    try {
      await vault.updateSettings(next);
    } catch (e) {
      // Only realistic cause is the vault locking mid-drag. The width reverts
      // on its own once dragWidth clears; say why rather than revert silently.
      showToast(`Could not save sidebar width: ${errorMessage(e)}`, true);
    }
  }

  function startResize(e: PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startWidth = width;
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);

    const onMove = (ev: PointerEvent) => (dragWidth = clampWidth(startWidth + ev.clientX - startX));
    const onUp = async () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      const final = dragWidth;
      if (final === null) return; // press without movement
      await persistWidth(final);
      // Cleared only after the write lands, so the width never snaps back to
      // the old value for a frame.
      dragWidth = null;
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  }

  function connect(connectionId: string) {
    tabs.open(connectionId);
  }

  function openMenu(e: MouseEvent, node: TreeNode) {
    if (node.type === "connection") {
      const conn = vault.data?.connections[node.id];
      if (!conn) return;
      menu = {
        x: e.clientX,
        y: e.clientY,
        items: [
          { label: "Connect", action: () => connect(node.id) },
          { separator: true, label: "" },
          { label: "Edit…", action: () => (connectionDialog = { existing: conn, parent: null }) },
          { label: "Duplicate", action: () => void vault.duplicateConnection(node.id) },
          { separator: true, label: "" },
          {
            label: "Delete",
            danger: true,
            action: () =>
              (confirm = {
                title: "Delete connection",
                message: `Delete "${conn.name}"? This cannot be undone.`,
                action: () => void vault.deleteConnection(node.id),
              }),
          },
        ],
      };
    } else {
      const folder = node;
      menu = {
        x: e.clientX,
        y: e.clientY,
        items: [
          {
            label: "New connection…",
            action: () => (connectionDialog = { existing: null, parent: folder.id }),
          },
          {
            label: "New folder…",
            action: () => (folderDialog = { existing: null, parent: folder.id }),
          },
          { separator: true, label: "" },
          {
            label: "Edit…",
            action: () =>
              (folderDialog = {
                existing: { id: folder.id, name: folder.name, badge: folder.badge ?? null },
                parent: null,
              }),
          },
          { separator: true, label: "" },
          {
            label: "Delete",
            danger: true,
            action: () =>
              (confirm = {
                title: "Delete folder",
                message: `Delete folder "${folder.name}"? Its contents move up one level.`,
                action: () => void vault.deleteFolder(folder.id),
              }),
          },
        ],
      };
    }
  }

  function openBackgroundMenu(e: MouseEvent) {
    e.preventDefault();
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: "New connection…", action: () => (connectionDialog = { existing: null, parent: null }) },
        { label: "New folder…", action: () => (folderDialog = { existing: null, parent: null }) },
      ],
    };
  }

</script>

<aside class="sidebar" style:width="{width}px">
  <SidebarTree
    onbackgroundmenu={openBackgroundMenu}
    onconnect={connect}
    onmenu={openMenu}
    onnewconnection={() => (connectionDialog = { existing: null, parent: null })}
    onnewfolder={() => (folderDialog = { existing: null, parent: null })}
  />

  <div
    class="resizer"
    role="separator"
    aria-orientation="vertical"
    title="Drag to resize · double-click to reset"
    onpointerdown={startResize}
    ondblclick={() => void persistWidth(WIDTH_DEFAULT)}
  ></div>
</aside>

{#if menu}
  <ContextMenu x={menu.x} y={menu.y} items={menu.items} onclose={() => (menu = null)} />
{/if}

{#if connectionDialog}
  <ConnectionDialog
    existing={connectionDialog.existing}
    parentFolder={connectionDialog.parent}
    onclose={() => (connectionDialog = null)}
  />
{/if}

{#if folderDialog}
  <FolderDialog
    existing={folderDialog.existing}
    parentFolder={folderDialog.parent}
    onclose={() => (folderDialog = null)}
  />
{/if}

{#if confirm}
  <ConfirmDialog
    title={confirm.title}
    message={confirm.message}
    onconfirm={confirm.action}
    onclose={() => (confirm = null)}
  />
{/if}

<style>
  /* Width is driven by the vault setting; never shrink or grow to fit, or the
     dragged edge would not match the stored value. */
  .sidebar {
    position: relative;
    flex: 0 0 auto;
    background: var(--bg-1);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  /* Straddles the right border so the grab area is comfortable without
     stealing a visible pixel from either side. */
  .resizer {
    position: absolute;
    top: 0;
    right: -2px;
    width: 5px;
    height: 100%;
    z-index: 5;
    cursor: col-resize;
  }

</style>
