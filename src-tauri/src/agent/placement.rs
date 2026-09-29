//! Where a connection the agent adds goes in the sidebar tree: a folder
//! path like `Clients/Acme`, found by name or created on the way.

use serverus_domain::agent::access::{created_connection_level, effective_level, AccessLevel};

use crate::error::{AppError, AppResult};
use crate::vault::model::{AgentAccessLevel, Connection, TreeNode, VaultPayload};
use crate::vault::tree;

/// A connection the agent added.
#[derive(Debug)]
pub struct Added {
    pub id: String,
    /// `Folder/Sub/Name`.
    pub path: String,
    /// Folders created on the way.
    pub created: Vec<String>,
}

/// The effective level a connection added to `folder` would get, without
/// changing anything (the folders it would create inherit).
pub fn preview_level(
    tree: &[TreeNode],
    folder: &str,
    full_access: bool,
) -> Result<AccessLevel, String> {
    let mut preview = tree.to_vec();
    let placement = ensure_folder_path(&mut preview, folder)?;
    Ok(placement.new_connection_level(full_access).1)
}

/// Put `connection` into `folder` (created as needed). Its own agent access
/// follows the domain rule: inherit the folder's level, but start at "ask"
/// where inheriting would hide it from the agent that added it. `expected`
/// is the effective level the user's approval was based on: if the folders
/// changed meanwhile and the level would differ, nothing is added.
pub fn add_connection(
    payload: &mut VaultPayload,
    folder: &str,
    mut connection: Connection,
    full_access: bool,
    expected: AccessLevel,
) -> AppResult<Added> {
    let placement = ensure_folder_path(&mut payload.tree, folder).map_err(AppError::Other)?;
    let (own, level) = placement.new_connection_level(full_access);
    if level != expected {
        return Err(AppError::Other(
            "The folder's AI access changed while this was being approved; nothing was added. Try again."
                .into(),
        ));
    }
    connection.agent_access = own.map(Into::into);
    let id = uuid::Uuid::new_v4().to_string();
    let path = placed_path(folder, &connection.name);
    payload.connections.insert(id.clone(), connection);
    tree::insert_node(
        &mut payload.tree,
        placement.folder_id.as_deref(),
        TreeNode::Connection { id: id.clone() },
    )?;
    Ok(Added {
        id,
        path,
        created: placement.created,
    })
}

fn placed_path(folder: &str, name: &str) -> String {
    folder
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .chain(std::iter::once(name))
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Debug, Default)]
pub struct Placement {
    /// The innermost folder, `None` for the root.
    pub folder_id: Option<String>,
    /// The folders' own access levels, innermost first.
    pub ancestors: Vec<Option<AgentAccessLevel>>,
    /// Paths of the folders created on the way.
    pub created: Vec<String>,
}

impl Placement {
    /// A new connection's own stored level here and its effective level.
    pub fn new_connection_level(&self, full_access: bool) -> (Option<AccessLevel>, AccessLevel) {
        let ancestors = || self.ancestors.iter().map(|level| level.map(Into::into));
        let inherited = effective_level(None, ancestors(), full_access);
        let own = created_connection_level(inherited);
        (own, effective_level(own, ancestors(), full_access))
    }
}

/// Walk (and extend) `tree` along a `/`-separated folder path. Each step
/// takes a folder with exactly that name, else the only one matching it
/// case-insensitively, else a new folder. New folders inherit agent access.
pub fn ensure_folder_path(tree: &mut Vec<TreeNode>, path: &str) -> Result<Placement, String> {
    let parts: Vec<&str> = path
        .split('/')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    let mut placement = Placement::default();
    descend(tree, &parts, String::new(), &mut placement)?;
    placement.ancestors.reverse();
    Ok(placement)
}

fn descend(
    nodes: &mut Vec<TreeNode>,
    parts: &[&str],
    prefix: String,
    placement: &mut Placement,
) -> Result<(), String> {
    let Some((name, rest)) = parts.split_first() else {
        return Ok(());
    };
    let path = if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}/{name}")
    };
    let index = match find_folder(nodes, name)? {
        Some(index) => index,
        None => {
            nodes.push(TreeNode::Folder {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.to_string(),
                badge: None,
                children: Vec::new(),
                collapsed: false,
                agent_access: None,
            });
            placement.created.push(path.clone());
            nodes.len() - 1
        }
    };
    let TreeNode::Folder {
        id,
        children,
        agent_access,
        ..
    } = &mut nodes[index]
    else {
        unreachable!("find_folder returns folders only");
    };
    placement.folder_id = Some(id.clone());
    placement.ancestors.push(*agent_access);
    descend(children, rest, path, placement)
}

fn find_folder(nodes: &[TreeNode], wanted: &str) -> Result<Option<usize>, String> {
    let folders = || {
        nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| match node {
                TreeNode::Folder { name, .. } => Some((index, name.as_str())),
                TreeNode::Connection { .. } => None,
            })
    };
    if let Some((index, _)) = folders().find(|(_, name)| *name == wanted) {
        return Ok(Some(index));
    }
    let similar: Vec<usize> = folders()
        .filter(|(_, name)| name.eq_ignore_ascii_case(wanted))
        .map(|(index, _)| index)
        .collect();
    match similar.as_slice() {
        [] => Ok(None),
        [index] => Ok(Some(*index)),
        _ => Err(format!(
            "Several folders are named like `{wanted}`; use the exact name."
        )),
    }
}
