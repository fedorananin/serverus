use crate::agent::placement::{add_connection, ensure_folder_path, preview_level};
use crate::vault::model::{
    AgentAccessLevel, AuthConfig, AuthMethod, Connection, Protocol, TreeNode, VaultPayload,
};
use crate::vault::tree;
use serverus_domain::agent::access::AccessLevel;

fn folder(name: &str, level: Option<AgentAccessLevel>, children: Vec<TreeNode>) -> TreeNode {
    TreeNode::Folder {
        id: format!("id-{name}"),
        name: name.into(),
        badge: None,
        children,
        collapsed: false,
        agent_access: level,
    }
}

fn connection(name: &str) -> Connection {
    Connection {
        name: name.into(),
        badge: None,
        protocol: Protocol::Ssh,
        host: "10.0.0.5".into(),
        port: 22,
        auth: AuthConfig {
            method: AuthMethod::Password,
            username: "deploy".into(),
            password: Some("s3cret".into()),
            key_path: None,
            key_inline: None,
            key_passphrase: None,
        },
        jump_host: None,
        ftp: None,
        s3: None,
        remote_dir: None,
        local_dir: None,
        tunnels: vec![],
        disable_terminal: false,
        notes: String::new(),
        agent_access: None,
    }
}

#[test]
fn existing_folders_are_reused_and_missing_ones_created() {
    let mut tree = vec![folder(
        "Clients",
        Some(AgentAccessLevel::Full),
        vec![folder("Acme", None, vec![])],
    )];
    let placement = ensure_folder_path(&mut tree, " clients / Acme /New ").unwrap();
    assert_eq!(placement.created, vec!["clients/Acme/New".to_string()]);
    // Innermost first: New (inherit), Acme (inherit), Clients (full).
    assert_eq!(
        placement.ancestors,
        vec![None, None, Some(AgentAccessLevel::Full)]
    );
    let TreeNode::Folder { children, .. } = &tree[0] else {
        panic!()
    };
    let TreeNode::Folder { children, .. } = &children[0] else {
        panic!()
    };
    assert!(matches!(&children[0], TreeNode::Folder { name, id, .. }
        if name == "New" && Some(id) == placement.folder_id.as_ref()));
}

#[test]
fn an_empty_path_is_the_top_level() {
    let mut tree = vec![];
    let placement = ensure_folder_path(&mut tree, "  ").unwrap();
    assert!(placement.folder_id.is_none());
    assert!(tree.is_empty());
}

#[test]
fn ambiguous_folder_names_are_refused() {
    let mut tree = vec![folder("prod", None, vec![]), folder("PROD", None, vec![])];
    assert!(ensure_folder_path(&mut tree, "Prod").is_err());
    // An exact match is never ambiguous.
    assert!(ensure_folder_path(&mut tree, "prod").is_ok());
}

#[test]
fn a_new_connection_inherits_its_folder_level() {
    let mut payload = VaultPayload {
        tree: vec![folder("Prod", Some(AgentAccessLevel::ReadOnly), vec![])],
        ..VaultPayload::default()
    };
    let added = add_connection(
        &mut payload,
        "Prod",
        connection("web"),
        false,
        AccessLevel::ReadOnly,
    )
    .unwrap();
    assert_eq!(added.path, "Prod/web");
    let stored = &payload.connections[&added.id];
    assert_eq!(stored.agent_access, None, "inherits read-only");
    assert_eq!(stored.auth.password.as_deref(), Some("s3cret"));
    let tree_copy = payload.tree.clone();
    tree::validate_tree(&payload, &tree_copy).unwrap();
}

#[test]
fn a_connection_that_would_be_hidden_starts_at_ask() {
    let mut payload = VaultPayload::default();
    let added = add_connection(
        &mut payload,
        "New/Place",
        connection("db"),
        false,
        AccessLevel::Ask,
    )
    .unwrap();
    assert_eq!(
        added.created,
        vec!["New".to_string(), "New/Place".to_string()]
    );
    assert_eq!(
        payload.connections[&added.id].agent_access,
        Some(AgentAccessLevel::Ask)
    );
    // Full access for everything makes it visible anyway: plain inherit.
    let added =
        add_connection(&mut payload, "", connection("db2"), true, AccessLevel::Full).unwrap();
    assert_eq!(payload.connections[&added.id].agent_access, None);
    assert_eq!(added.path, "db2");
}

#[test]
fn the_level_is_previewed_without_changes_and_checked_on_commit() {
    let tree = vec![folder("Ops", Some(AgentAccessLevel::Full), vec![])];
    assert_eq!(
        preview_level(&tree, "Ops/New", false),
        Ok(AccessLevel::Full)
    );
    assert_eq!(
        preview_level(&tree, "Elsewhere", false),
        Ok(AccessLevel::Ask)
    );
    assert_eq!(tree.len(), 1, "a preview creates no folders");

    // Approved as "ask", but the folder became "full" meanwhile: refused.
    let mut payload = VaultPayload {
        tree,
        ..VaultPayload::default()
    };
    let refused = add_connection(
        &mut payload,
        "Ops",
        connection("web"),
        false,
        AccessLevel::Ask,
    );
    assert!(refused.is_err());
    assert!(payload.connections.is_empty());
}
