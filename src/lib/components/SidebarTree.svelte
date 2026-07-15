<script lang="ts">
  import type { TreeNode } from "$lib/api";
  import { dnd } from "$lib/stores/dnd.svelte";
  import { vault } from "$lib/stores/vault.svelte";
  import {
    appendNode,
    cloneTree,
    containsNode,
    extractNode,
    findFolder,
    insertAfterNode,
    insertBeforeNode,
  } from "$lib/tree";
  import BadgeIcon from "./BadgeIcon.svelte";
  import SidebarNode from "./SidebarNode.svelte";

  interface Props {
    onbackgroundmenu: (event: MouseEvent) => void;
    onconnect: (connectionId: string) => void;
    onmenu: (event: MouseEvent, node: TreeNode) => void;
    onnewconnection: () => void;
    onnewfolder: () => void;
  }

  let { onbackgroundmenu, onconnect, onmenu, onnewconnection, onnewfolder }: Props = $props();

  let search = $state("");
  let selectedId = $state<string | null>(null);

  const tree = $derived(vault.data?.tree ?? []);

  // Live search over name/host (SPEC §5.1) — flat result list.
  const searchResults = $derived.by(() => {
    const q = search.trim().toLowerCase();
    if (!q || !vault.data) return null;
    return Object.values(vault.data.connections)
      .filter((connection) =>
        connection.name.toLowerCase().includes(q) || connection.host.toLowerCase().includes(q),
      )
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

<div class="search">
  <input type="text" placeholder="🔍 Search name or host" bind:value={search} />
</div>

<div
  class="tree"
  role="tree"
  tabindex="-1"
  oncontextmenu={onbackgroundmenu}
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
    {#each searchResults as connection (connection.id)}
      <div
        class="result"
        role="treeitem"
        aria-label={connection.name}
        aria-selected={selectedId === connection.id}
        tabindex="-1"
        class:selected={selectedId === connection.id}
        onclick={() => (selectedId = connection.id)}
        ondblclick={() => onconnect(connection.id)}
        onkeydown={(event) => event.key === "Enter" && onconnect(connection.id)}
        oncontextmenu={(event) => {
          event.preventDefault();
          event.stopPropagation();
          selectedId = connection.id;
          onmenu(event, { type: "connection", id: connection.id });
        }}
      >
        <BadgeIcon
          badge={connection.badge}
          fallback={connection.protocol === "ssh" ? "🖥" : connection.protocol === "s3" ? "🪣" : "📦"}
        />
        <span class="name">{connection.name}</span>
        <span class="host mono">{connection.host}</span>
      </div>
    {:else}
      <div class="empty">No matches</div>
    {/each}
  {:else}
    {#each tree as node (node.id)}
      <SidebarNode
        {node}
        depth={0}
        {selectedId}
        ontoggle={(id) => void toggle(id)}
        onselect={(id) => (selectedId = id)}
        onconnect={onconnect}
        onmenu={onmenu}
        ondragfinish={(id) => void finishDrag(id)}
      />
    {:else}
      <div class="empty" role="status">
        No connections yet.<br />Right-click here or press the button below.
      </div>
    {/each}
  {/if}
</div>

<div class="actions">
  <button onclick={onnewconnection}>+ Connection</button>
  <button class="icon" title="New folder" aria-label="New folder" onclick={onnewfolder}>📁+</button>
</div>

<style>
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
