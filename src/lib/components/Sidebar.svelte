<script lang="ts">
  import type { Badge, PublicConnection, Settings, TreeNode } from "$lib/api";
  import { errorMessage } from "$lib/api";
  import { vault } from "$lib/stores/vault.svelte";
  import { tabs } from "$lib/stores/tabs.svelte";
  import {
    appendNode,
    cloneTree,
    containsNode,
    extractNode,
    findFolder,
    insertAfterNode,
    insertBeforeNode,
  } from "$lib/tree";
  import { dnd } from "$lib/stores/dnd.svelte";
  import SidebarNode from "./SidebarNode.svelte";
  import BadgeIcon from "./BadgeIcon.svelte";
  import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";
  import ConnectionDialog from "./ConnectionDialog.svelte";
  import FolderDialog from "./FolderDialog.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { showToast } from "./Toasts.svelte";

  let search = $state("");
  let selectedId = $state<string | null>(null);

  let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);
  let connectionDialog = $state<{ existing: PublicConnection | null; parent: string | null } | null>(null);
  let folderDialog = $state<{
    existing: { id: string; name: string; badge: Badge | null } | null;
    parent: string | null;
  } | null>(null);
  let confirm = $state<{ title: string; message: string; action: () => void } | null>(null);

  const tree = $derived(vault.data?.tree ?? []);

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

  // Live search over name/host (SPEC §5.1) — flat result list.
  const searchResults = $derived.by(() => {
    const q = search.trim().toLowerCase();
    if (!q || !vault.data) return null;
    return Object.values(vault.data.connections)
      .filter((c) => c.name.toLowerCase().includes(q) || c.host.toLowerCase().includes(q))
      .sort((a, b) => a.name.localeCompare(b.name));
  });

  // Disclosure state lives on the folder node, so it survives restarts and
  // travels with the folder when it is moved or deleted.
  async function toggle(folderId: string) {
    const next = cloneTree($state.snapshot(tree) as TreeNode[]);
    const folder = findFolder(next, folderId);
    if (!folder) return;
    folder.collapsed = !folder.collapsed;
    await vault.updateTree(next);
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

  /** Apply a finished tree drag: reads the zone target set while hovering.
      Backend re-validates the tree, so nothing here can corrupt the vault. */
  async function finishDrag(draggedId: string) {
    const target = dnd.treeTarget;
    const rootDrop = dnd.treeRootHover;
    const next = cloneTree($state.snapshot(tree) as TreeNode[]);
    const dragged = extractNode(next, draggedId);
    if (!dragged) return;

    if (target && target.id !== draggedId) {
      // A folder can't be dropped into itself or its descendants.
      if (containsNode(dragged, target.id)) return;
      const placed =
        target.zone === "into"
          ? appendNode(next, target.id, dragged)
          : target.zone === "before"
            ? insertBeforeNode(next, target.id, dragged)
            : insertAfterNode(next, target.id, dragged);
      if (!placed) next.push(dragged);
      if (target.zone === "into") {
        // Reveal the folder we just dropped into.
        const folder = findFolder(next, target.id);
        if (folder) folder.collapsed = false;
      }
    } else if (rootDrop) {
      next.push(dragged); // dropped on empty tree space → end of root
    } else {
      return; // released outside the sidebar — no move
    }
    await vault.updateTree(next);
  }
</script>

<aside class="sidebar" style:width="{width}px">
  <div class="search">
    <input type="text" placeholder="🔍 Search name or host" bind:value={search} />
  </div>

  <div
    class="tree"
    role="tree"
    tabindex="-1"
    oncontextmenu={openBackgroundMenu}
    onpointermove={() => {
      // Row handlers overwrite this with a precise target; reaching here
      // means the pointer is over empty tree space.
      if (dnd.active?.kind === "tree-node") {
        dnd.treeRootHover = true;
        dnd.treeTarget = null;
      }
    }}
    onpointerleave={() => (dnd.treeRootHover = false)}
  >
    {#if searchResults}
      {#each searchResults as conn (conn.id)}
        <div
          class="result"
          role="treeitem"
          aria-selected={selectedId === conn.id}
          tabindex="-1"
          class:selected={selectedId === conn.id}
          onclick={() => (selectedId = conn.id)}
          ondblclick={() => connect(conn.id)}
          onkeydown={(e) => e.key === "Enter" && connect(conn.id)}
          oncontextmenu={(e) => {
            e.preventDefault();
            e.stopPropagation();
            selectedId = conn.id;
            openMenu(e, { type: "connection", id: conn.id });
          }}
        >
          <BadgeIcon
            badge={conn.badge}
            fallback={conn.protocol === "ssh" ? "🖥" : conn.protocol === "s3" ? "🪣" : "📦"}
          />
          <span class="name">{conn.name}</span>
          <span class="host mono">{conn.host}</span>
        </div>
      {:else}
        <div class="empty">No matches</div>
      {/each}
    {:else}
      {#each tree as node (node.type === "folder" ? node.id : node.id)}
        <SidebarNode
          {node}
          depth={0}
          {selectedId}
          ontoggle={(id) => void toggle(id)}
          onselect={(id) => (selectedId = id)}
          onconnect={connect}
          onmenu={openMenu}
          ondragfinish={(id) => void finishDrag(id)}
        />
      {:else}
        <div class="empty">
          No connections yet.<br />Right-click here or press the button below.
        </div>
      {/each}
    {/if}
  </div>

  <div class="actions">
    <button onclick={() => (connectionDialog = { existing: null, parent: null })}>+ Connection</button>
    <button
      class="icon"
      title="New folder"
      aria-label="New folder"
      onclick={() => (folderDialog = { existing: null, parent: null })}>📁+</button
    >
  </div>

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

  .search {
    padding: 8px;
  }

  .search input {
    width: 100%;
    font-size: 12px;
  }

  .tree {
    flex: 1;
    overflow-y: auto;
    padding: 2px 0 8px;
  }

  .result {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 12px;
    border-radius: var(--radius);
    margin: 0 4px;
  }

  .result:hover {
    background: var(--bg-2);
  }

  .result.selected {
    background: var(--bg-3);
  }

  .result .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .result .host {
    margin-left: auto;
    font-size: 10px;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .empty {
    color: var(--text-2);
    font-size: 12px;
    text-align: center;
    padding: 24px 12px;
  }

  .actions {
    display: flex;
    gap: 6px;
    padding: 8px;
    border-top: 1px solid var(--border);
  }

  .actions button {
    flex: 1;
    font-size: 12px;
  }

  .actions .icon {
    flex: 0 0 auto;
  }
</style>
