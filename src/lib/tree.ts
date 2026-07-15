// Client-side tree helpers for drag & drop rearrangement. The backend
// re-validates every tree we send, so bugs here can't corrupt the vault.

import type { TreeNode } from "$lib/api";

export function nodeId(node: TreeNode): string {
  return node.type === "folder" ? node.id : node.id;
}

export function cloneTree(tree: TreeNode[]): TreeNode[] {
  return structuredClone(tree);
}

/** Remove the node with `id` (folder or connection ref) and return it. */
export function extractNode(tree: TreeNode[], id: string): TreeNode | null {
  for (let i = 0; i < tree.length; i++) {
    const node = tree[i];
    if (nodeId(node) === id) {
      tree.splice(i, 1);
      return node;
    }
    if (node.type === "folder") {
      const found = extractNode(node.children ?? [], id);
      if (found) return found;
    }
  }
  return null;
}

/** The folder node `folderId`, at any depth — returned for mutation in place. */
export function findFolder(
  tree: TreeNode[],
  folderId: string,
): Extract<TreeNode, { type: "folder" }> | null {
  for (const node of tree) {
    if (node.type !== "folder") continue;
    if (node.id === folderId) return node;
    const found = findFolder(node.children ?? [], folderId);
    if (found) return found;
  }
  return null;
}

/** True if `maybeChild` is inside the folder `folderId` (or is it). */
export function containsNode(node: TreeNode, id: string): boolean {
  if (nodeId(node) === id) return true;
  if (node.type === "folder") {
    return (node.children ?? []).some((c) => containsNode(c, id));
  }
  return false;
}

/** Append `node` into folder `folderId`, or root when null. */
export function appendNode(tree: TreeNode[], folderId: string | null, node: TreeNode): boolean {
  if (folderId === null) {
    tree.push(node);
    return true;
  }
  for (const candidate of tree) {
    if (candidate.type === "folder") {
      if (candidate.id === folderId) {
        (candidate.children ??= []).push(node);
        return true;
      }
      if (appendNode(candidate.children ?? [], folderId, node)) return true;
    }
  }
  return false;
}

/** Insert `node` immediately after the node `anchorId` (same parent). */
export function insertAfterNode(tree: TreeNode[], anchorId: string, node: TreeNode): boolean {
  for (let i = 0; i < tree.length; i++) {
    if (nodeId(tree[i]) === anchorId) {
      tree.splice(i + 1, 0, node);
      return true;
    }
    const candidate = tree[i];
    if (candidate.type === "folder" && insertAfterNode(candidate.children ?? [], anchorId, node)) {
      return true;
    }
  }
  return false;
}

/** Insert `node` immediately before the node `anchorId` (same parent). */
export function insertBeforeNode(tree: TreeNode[], anchorId: string, node: TreeNode): boolean {
  for (let i = 0; i < tree.length; i++) {
    if (nodeId(tree[i]) === anchorId) {
      tree.splice(i, 0, node);
      return true;
    }
    const candidate = tree[i];
    if (candidate.type === "folder" && insertBeforeNode(candidate.children ?? [], anchorId, node)) {
      return true;
    }
  }
  return false;
}
