//! Sidebar tree operations: validation and structural edits (SPEC §3, §5.1).
//!
//! The UI owns visual arrangement (drag & drop sends a whole new tree); the
//! backend validates every incoming tree against the connections map so a
//! buggy frontend can never corrupt the vault.

use std::collections::HashSet;

use crate::error::{AppError, AppResult};
use crate::vault::model::{TreeNode, VaultPayload};

/// Check invariants: unique folder ids, unique connection refs, every
/// connection ref resolves, no unknown connections dropped silently.
pub fn validate_tree(payload: &VaultPayload, tree: &[TreeNode]) -> AppResult<()> {
    let mut folder_ids = HashSet::new();
    let mut conn_ids = HashSet::new();
    walk(tree, &mut |node| match node {
        TreeNode::Folder { id, .. } => {
            if !folder_ids.insert(id.clone()) {
                return Err(AppError::Other(format!("duplicate folder id {id}")));
            }
            Ok(())
        }
        TreeNode::Connection { id } => {
            if !conn_ids.insert(id.clone()) {
                return Err(AppError::Other(format!("duplicate connection ref {id}")));
            }
            if !payload.connections.contains_key(id) {
                return Err(AppError::Other(format!("unknown connection {id}")));
            }
            Ok(())
        }
    })?;
    // Every stored connection must stay reachable from the tree.
    for id in payload.connections.keys() {
        if !conn_ids.contains(id) {
            return Err(AppError::Other(format!(
                "connection {id} missing from tree"
            )));
        }
    }
    Ok(())
}

fn walk(nodes: &[TreeNode], f: &mut impl FnMut(&TreeNode) -> AppResult<()>) -> AppResult<()> {
    for node in nodes {
        f(node)?;
        if let TreeNode::Folder { children, .. } = node {
            walk(children, f)?;
        }
    }
    Ok(())
}

/// Remove every node matching `pred`; children of removed folders are lifted
/// to the removed folder's position (delete folder keeps its contents).
pub fn remove_nodes(nodes: &mut Vec<TreeNode>, pred: &impl Fn(&TreeNode) -> bool, lift: bool) {
    let mut i = 0;
    while i < nodes.len() {
        if pred(&nodes[i]) {
            let removed = nodes.remove(i);
            if lift {
                if let TreeNode::Folder { children, .. } = removed {
                    for (offset, child) in children.into_iter().enumerate() {
                        nodes.insert(i + offset, child);
                    }
                }
            }
        } else {
            if let TreeNode::Folder { children, .. } = &mut nodes[i] {
                remove_nodes(children, pred, lift);
            }
            i += 1;
        }
    }
}

/// Find a folder by id anywhere in the tree.
pub fn find_folder_mut<'a>(
    nodes: &'a mut [TreeNode],
    folder_id: &str,
) -> Option<&'a mut Vec<TreeNode>> {
    for node in nodes {
        if let TreeNode::Folder { id, children, .. } = node {
            if id == folder_id {
                return Some(children);
            }
            if let Some(found) = find_folder_mut(children, folder_id) {
                return Some(found);
            }
        }
    }
    None
}

/// Append a node at the end of `parent` (or the root when `parent` is None).
pub fn insert_node(
    tree: &mut Vec<TreeNode>,
    parent: Option<&str>,
    node: TreeNode,
) -> AppResult<()> {
    match parent {
        None => {
            tree.push(node);
            Ok(())
        }
        Some(folder_id) => match find_folder_mut(tree, folder_id) {
            Some(children) => {
                children.push(node);
                Ok(())
            }
            None => Err(AppError::Other(format!("folder {folder_id} not found"))),
        },
    }
}

/// Insert `node` right after the connection node `after_id`, wherever it
/// lives. Falls back to appending at the root when the anchor is missing.
pub fn insert_after(tree: &mut Vec<TreeNode>, after_id: &str, node: TreeNode) {
    fn try_insert(nodes: &mut Vec<TreeNode>, after_id: &str, node: &mut Option<TreeNode>) {
        let mut i = 0;
        while i < nodes.len() {
            if matches!(&nodes[i], TreeNode::Connection { id } if id == after_id) {
                nodes.insert(i + 1, node.take().unwrap());
                return;
            }
            if let TreeNode::Folder { children, .. } = &mut nodes[i] {
                try_insert(children, after_id, node);
                if node.is_none() {
                    return;
                }
            }
            i += 1;
        }
    }
    let mut slot = Some(node);
    try_insert(tree, after_id, &mut slot);
    if let Some(node) = slot {
        tree.push(node);
    }
}

/// Update a folder's name/badge in place.
pub fn update_folder(
    tree: &mut [TreeNode],
    folder_id: &str,
    name: String,
    badge: Option<crate::vault::model::Badge>,
) -> AppResult<()> {
    let mut updated = false;
    update_folder_walk(tree, folder_id, &mut |n, b| {
        *n = name.clone();
        *b = badge.clone();
        updated = true;
    });
    if updated {
        Ok(())
    } else {
        Err(AppError::Other(format!("folder {folder_id} not found")))
    }
}

fn update_folder_walk(
    nodes: &mut [TreeNode],
    folder_id: &str,
    f: &mut impl FnMut(&mut String, &mut Option<crate::vault::model::Badge>),
) {
    for node in nodes {
        if let TreeNode::Folder {
            id,
            name,
            badge,
            children,
            ..
        } = node
        {
            if id == folder_id {
                f(name, badge);
                return;
            }
            update_folder_walk(children, folder_id, f);
        }
    }
}

#[cfg(test)]
mod tests;
