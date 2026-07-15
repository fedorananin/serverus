//! Config import: merge a JSON config (a Serverus export, or a hand-written
//! file following docs/CONFIG_FORMAT.md) into the unlocked vault.
//!
//! Semantics: connections are upserted by id — secrets already stored in the
//! vault survive when the file omits them, and a file MAY carry secrets
//! (password / key_inline / key_passphrase) for a one-time migration, since
//! they end up inside the encrypted vault anyway. The imported tree is
//! appended to the existing one; nodes the import re-places are first removed
//! so re-importing the same file does not duplicate anything. Known hosts
//! merge with existing entries winning (the user already verified those).
//! Settings, when present, replace the current ones wholesale.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;

use crate::error::{AppError, AppResult};
use crate::vault::model::{
    AuthConfig, AuthMethod, Badge, Connection, FtpOptions, Protocol, S3Options, Settings, TreeNode,
    TunnelConfig, VaultPayload,
};
use crate::vault::tree;

/// Top-level import file. Every section is optional so a hand-written file
/// can stay minimal; unknown fields (e.g. the `has_password` flags an export
/// carries) are ignored.
#[derive(Debug, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    tree: Vec<ImportTreeNode>,
    #[serde(default)]
    connections: HashMap<String, ImportConnection>,
    #[serde(default)]
    known_hosts: HashMap<String, String>,
    #[serde(default)]
    settings: Option<Settings>,
}

/// Like [`TreeNode`], but a folder id is optional (assigned when missing).
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ImportTreeNode {
    Folder {
        #[serde(default)]
        id: Option<String>,
        name: String,
        #[serde(default)]
        badge: Option<Badge>,
        #[serde(default)]
        children: Vec<ImportTreeNode>,
    },
    Connection {
        id: String,
    },
}

/// Like [`Connection`], but only `protocol` and `host` are required.
#[derive(Debug, Deserialize)]
struct ImportConnection {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    badge: Option<Badge>,
    protocol: Protocol,
    host: String,
    #[serde(default)]
    port: Option<u16>,
    #[serde(default)]
    auth: ImportAuth,
    #[serde(default)]
    jump_host: Option<String>,
    #[serde(default)]
    ftp: Option<FtpOptions>,
    #[serde(default)]
    s3: Option<S3Options>,
    #[serde(default)]
    remote_dir: Option<String>,
    #[serde(default)]
    local_dir: Option<String>,
    #[serde(default)]
    tunnels: Vec<TunnelConfig>,
    #[serde(default)]
    disable_terminal: bool,
    #[serde(default)]
    notes: String,
}

#[derive(Debug, Default, Deserialize)]
struct ImportAuth {
    #[serde(default)]
    method: Option<AuthMethod>,
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    key_path: Option<String>,
    #[serde(default)]
    key_inline: Option<String>,
    #[serde(default)]
    key_passphrase: Option<String>,
}

impl ImportConnection {
    /// Build the stored connection, keeping `old`'s secrets where the file
    /// has none (an export never carries secrets).
    fn into_connection(self, old: Option<&Connection>) -> Connection {
        let method = self.auth.method.unwrap_or({
            if self.auth.key_inline.is_some() || self.auth.key_path.is_some() {
                AuthMethod::Key
            } else {
                AuthMethod::Password
            }
        });
        let old_auth = old.map(|c| &c.auth);
        let keep = |new: Option<String>, old: Option<&String>| new.or_else(|| old.cloned());
        Connection {
            name: self.name.unwrap_or_else(|| self.host.clone()),
            badge: self.badge,
            protocol: self.protocol,
            port: self.port.unwrap_or(match self.protocol {
                Protocol::Ssh => 22,
                Protocol::Ftp => 21,
                Protocol::S3 => 443,
            }),
            host: self.host,
            auth: AuthConfig {
                method,
                username: self.auth.username,
                password: keep(
                    self.auth.password,
                    old_auth.and_then(|a| a.password.as_ref()),
                ),
                key_path: self.auth.key_path,
                key_inline: keep(
                    self.auth.key_inline,
                    old_auth.and_then(|a| a.key_inline.as_ref()),
                ),
                key_passphrase: keep(
                    self.auth.key_passphrase,
                    old_auth.and_then(|a| a.key_passphrase.as_ref()),
                ),
            },
            jump_host: self.jump_host,
            ftp: self.ftp,
            s3: self.s3,
            remote_dir: self.remote_dir,
            local_dir: self.local_dir,
            tunnels: self.tunnels,
            disable_terminal: self.disable_terminal,
            notes: self.notes,
        }
    }
}

/// Convert the imported tree, assigning ids to folders that lack one and
/// dropping connection refs that don't resolve; collects what it references.
fn convert_tree(
    nodes: Vec<ImportTreeNode>,
    known: &HashMap<String, Connection>,
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
                    id,
                    name,
                    badge,
                    children: convert_tree(children, known, conn_refs, folder_ids),
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

fn collect_conn_refs(nodes: &[TreeNode], out: &mut HashSet<String>) {
    for node in nodes {
        match node {
            TreeNode::Connection { id } => {
                out.insert(id.clone());
            }
            TreeNode::Folder { children, .. } => collect_conn_refs(children, out),
        }
    }
}

/// Merge `json` into `payload`. Returns the number of imported connections.
pub fn apply(payload: &mut VaultPayload, json: &str) -> AppResult<u32> {
    let file: ConfigFile = serde_json::from_str(json)
        .map_err(|e| AppError::Other(format!("invalid config JSON: {e}")))?;
    if file.connections.is_empty()
        && file.tree.is_empty()
        && file.known_hosts.is_empty()
        && file.settings.is_none()
    {
        return Err(AppError::Other(
            "the file contains nothing to import".into(),
        ));
    }

    // Work on a copy so a validation failure leaves the vault untouched.
    let mut next = payload.clone();

    let mut imported = 0u32;
    for (id, conn) in file.connections {
        if id.trim().is_empty() {
            return Err(AppError::Other("connection with an empty id".into()));
        }
        let merged = conn.into_connection(next.connections.get(&id));
        next.connections.insert(id, merged);
        imported += 1;
    }

    // Imported jump-host refs must resolve; detach the ones that don't.
    let ids: HashSet<String> = next.connections.keys().cloned().collect();
    for conn in next.connections.values_mut() {
        if let Some(jump) = &conn.jump_host {
            if !ids.contains(jump) {
                conn.jump_host = None;
            }
        }
    }

    // Merge the tree: remove existing nodes the import re-places (so the
    // same file can be imported twice without duplicates), then append.
    let mut conn_refs = HashSet::new();
    let mut folder_ids = HashSet::new();
    let incoming = convert_tree(
        file.tree,
        &next.connections,
        &mut conn_refs,
        &mut folder_ids,
    );
    tree::remove_nodes(
        &mut next.tree,
        &|n| matches!(n, TreeNode::Connection { id } if conn_refs.contains(id)),
        false,
    );
    tree::remove_nodes(
        &mut next.tree,
        &|n| matches!(n, TreeNode::Folder { id, .. } if folder_ids.contains(id)),
        true, // lift children the import didn't claim
    );
    next.tree.extend(incoming);

    // Every stored connection must stay reachable from the tree — append the
    // ones the imported tree didn't place as loose nodes at the root.
    let mut reachable = HashSet::new();
    collect_conn_refs(&next.tree, &mut reachable);
    let mut orphans: Vec<&String> = ids.iter().filter(|id| !reachable.contains(*id)).collect();
    orphans.sort(); // deterministic order
    for id in orphans {
        next.tree.push(TreeNode::Connection { id: id.clone() });
    }

    // Existing known-host entries win — the user already verified those keys.
    for (host, key) in file.known_hosts {
        next.known_hosts.entry(host).or_insert(key);
    }

    if let Some(settings) = file.settings {
        next.settings = settings;
    }

    let tree_copy = next.tree.clone();
    tree::validate_tree(&next, &tree_copy)?;
    *payload = next;
    Ok(imported)
}

#[cfg(test)]
mod tests;
