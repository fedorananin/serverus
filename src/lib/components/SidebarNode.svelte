<script lang="ts">
  import type { TreeNode } from "$lib/api";
  import { vault } from "$lib/stores/vault.svelte";
  import { dnd, type TreeZone } from "$lib/stores/dnd.svelte";
  import BadgeIcon from "./BadgeIcon.svelte";
  import SidebarNode from "./SidebarNode.svelte";

  interface Props {
    node: TreeNode;
    depth: number;
    selectedId: string | null;
    ontoggle: (folderId: string) => void;
    onselect: (nodeId: string) => void;
    onconnect: (connectionId: string) => void;
    onmenu: (e: MouseEvent, node: TreeNode) => void;
    ondragfinish: (draggedId: string) => void;
  }

  let {
    node,
    depth,
    selectedId,
    ontoggle,
    onselect,
    onconnect,
    onmenu,
    ondragfinish,
  }: Props = $props();

  const isFolder = $derived(node.type === "folder");
  const id = $derived(node.type === "folder" ? node.id : node.id);
  const isOpen = $derived(node.type === "folder" && !node.collapsed);
  const connection = $derived(
    node.type === "connection" ? (vault.data?.connections[node.id] ?? null) : null,
  );

  const displayName = $derived(
    node.type === "folder" ? node.name : (connection?.name ?? "?"),
  );

  // Drop hint for this row while a tree drag is in flight.
  const dropZone = $derived(
    dnd.active?.kind === "tree-node" && dnd.treeTarget?.id === id && dnd.active.id !== id
      ? dnd.treeTarget.zone
      : null,
  );

  function startDrag(e: PointerEvent) {
    if (e.button !== 0) return;
    // Text selection is disabled via CSS; no preventDefault (it can suppress
    // the click/dblclick that select and open connections).
    const draggedId = id;
    dnd.begin(e, { kind: "tree-node", id: draggedId }, displayName, () =>
      ondragfinish(draggedId),
    );
  }

  /** Zone by pointer position: folders take before/into/after, plain rows
      split into before/after — so any ordering is reachable (SPEC §5.1). */
  function zoneAt(e: PointerEvent): TreeZone {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const ratio = (e.clientY - rect.top) / rect.height;
    if (isFolder) {
      if (ratio < 0.3) return "before";
      if (ratio > 0.7) return "after";
      return "into";
    }
    return ratio < 0.5 ? "before" : "after";
  }

  function trackTarget(e: PointerEvent) {
    if (dnd.active?.kind !== "tree-node") return;
    e.stopPropagation(); // the tree background handler must not clear this
    dnd.treeTarget = { id, zone: zoneAt(e) };
    dnd.treeRootHover = false;
  }

  function clearTarget() {
    if (dnd.treeTarget?.id === id) dnd.treeTarget = null;
  }
</script>

<div
  class="row"
  class:selected={selectedId === id}
  class:drop-into={dropZone === "into"}
  class:drop-before={dropZone === "before"}
  class:drop-after={dropZone === "after"}
  style:padding-left="{10 + depth * 14}px"
  role="treeitem"
  aria-label={displayName}
  aria-selected={selectedId === id}
  aria-expanded={isFolder ? isOpen : undefined}
  tabindex="-1"
  onpointerdown={startDrag}
  onpointermove={trackTarget}
  onpointerleave={clearTarget}
  onclick={() => {
    onselect(id);
    if (isFolder) ontoggle(id);
  }}
  ondblclick={() => {
    if (node.type === "connection") onconnect(node.id);
  }}
  onkeydown={(e) => {
    if (e.key === "Enter") {
      if (node.type === "connection") onconnect(node.id);
      else ontoggle(id);
    }
  }}
  oncontextmenu={(e) => {
    e.preventDefault();
    e.stopPropagation();
    onselect(id);
    onmenu(e, node);
  }}
>
  {#if node.type === "folder"}
    <span class="chevron" class:open={isOpen}>▸</span>
    <BadgeIcon badge={node.badge} fallback={isOpen ? "📂" : "📁"} />
    <span class="name">{node.name}</span>
    <!-- The count helps decide whether a closed folder is worth opening;
         once it's open the contents are visible anyway. -->
    {#if !isOpen}
      <span class="count">{node.children?.length ?? 0}</span>
    {/if}
  {:else if connection}
    <span class="chevron"></span>
    <BadgeIcon
      badge={connection.badge}
      fallback={connection.protocol === "ssh" ? "🖥" : connection.protocol === "s3" ? "🪣" : "📦"}
    />
    <span class="name">{connection.name}</span>
    <!-- SSH with the terminal disabled is effectively an SFTP-only account. -->
    <span class="proto mono"
      >{connection.protocol === "ssh" && connection.disable_terminal
        ? "sftp"
        : connection.protocol}</span
    >
  {/if}
</div>

{#if isFolder && isOpen && node.type === "folder"}
  {#each node.children ?? [] as child (child.type === "folder" ? child.id : child.id)}
    <SidebarNode
      node={child}
      depth={depth + 1}
      {selectedId}
      {ontoggle}
      {onselect}
      {onconnect}
      {onmenu}
      {ondragfinish}
    />
  {/each}
{/if}

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px;
    border-radius: var(--radius);
    margin: 0 4px;
    cursor: default;
    border: 1px solid transparent;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
  }

  .row:hover {
    background: var(--bg-2);
  }

  .row.selected {
    background: var(--bg-3);
  }

  .row.drop-into {
    border-color: var(--accent);
    background: var(--accent-subtle);
  }

  .row.drop-before {
    border-top-color: var(--accent);
    border-radius: 0;
  }

  .row.drop-after {
    border-bottom-color: var(--accent);
    border-radius: 0;
  }

  .chevron {
    width: 12px;
    font-size: 11px;
    color: var(--text-1);
    transition: transform 0.12s;
    flex-shrink: 0;
    text-align: center;
  }

  .chevron.open {
    transform: rotate(90deg);
  }

  .name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .count,
  .proto {
    font-size: 10px;
    color: var(--text-2);
    flex-shrink: 0;
  }
</style>
