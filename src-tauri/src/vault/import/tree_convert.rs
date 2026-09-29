//! Imported-tree conversion: folder ids, reference validation, and the
//! agent access levels a re-imported folder keeps.

use std::collections::{HashMap, HashSet};

use super::ImportTreeNode;
use crate::vault::model::{AgentAccessLevel, Connection, TreeNode};

/// Convert the imported tree, assigning ids to folders that lack one and
/// dropping connection refs that don't resolve; collects what it references.
pub(super) fn convert_tree(
    nodes: Vec<ImportTreeNode>,
    known: &HashMap<String, Connection>,
    folder_access: &HashMap<String, AgentAccessLevel>,
    conn_refs: &mut HashSet<String>,
    folder_ids: &mut HashSet<String>,
) -> Vec<TreeNode> {
    nodes
        .into_iter()
        .filter_map(|node| match node {
            ImportTreeNode::Folder {
                id,
                name,
                badge,
                children,
            } => {
                let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                folder_ids.insert(id.clone());
                Some(TreeNode::Folder {
                    agent_access: folder_access.get(&id).copied(),
                    id,
                    name,
                    badge,
                    children: convert_tree(children, known, folder_access, conn_refs, folder_ids),
                    collapsed: false,
                })
            }
            ImportTreeNode::Connection { id } => {
                // A ref must resolve AND be unique — validate_tree rejects
                // duplicates, so silently keep only the first occurrence.
                if known.contains_key(&id) && conn_refs.insert(id.clone()) {
                    Some(TreeNode::Connection { id })
                } else {
                    None
                }
            }
        })
        .collect()
}

/// Agent access levels of the folders already in the vault — a re-imported
/// folder keeps the level the user gave it, a new one inherits.
pub(super) fn collect_folder_access(
    nodes: &[TreeNode],
    out: &mut HashMap<String, AgentAccessLevel>,
) {
    for node in nodes {
        if let TreeNode::Folder {
            id,
            children,
            agent_access,
            ..
        } = node
        {
            if let Some(level) = agent_access {
                out.insert(id.clone(), *level);
            }
            collect_folder_access(children, out);
        }
    }
}

pub(super) fn collect_conn_refs(nodes: &[TreeNode], out: &mut HashSet<String>) {
    for node in nodes {
        match node {
            TreeNode::Connection { id } => {
                out.insert(id.clone());
            }
            TreeNode::Folder { children, .. } => collect_conn_refs(children, out),
        }
    }
}
