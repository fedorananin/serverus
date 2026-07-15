use super::*;
use crate::vault::model::{AuthConfig, AuthMethod, Badge, BadgeKind, Connection, Protocol};

fn conn(name: &str) -> Connection {
    Connection {
        name: name.into(),
        badge: None,
        protocol: Protocol::Ssh,
        host: "h".into(),
        port: 22,
        auth: AuthConfig {
            method: AuthMethod::Agent,
            username: "u".into(),
            password: None,
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
    }
}

fn folder(id: &str, children: Vec<TreeNode>) -> TreeNode {
    TreeNode::Folder {
        id: id.into(),
        name: id.into(),
        badge: Some(Badge {
            kind: BadgeKind::Emoji,
            value: "📁".into(),
        }),
        children,
        collapsed: false,
    }
}

fn payload_with(tree: Vec<TreeNode>, conns: &[&str]) -> VaultPayload {
    let mut p = VaultPayload {
        tree,
        ..Default::default()
    };
    for id in conns {
        p.connections.insert(id.to_string(), conn(id));
    }
    p
}

#[test]
fn validates_good_tree() {
    let tree = vec![
        folder("f1", vec![TreeNode::Connection { id: "c1".into() }]),
        TreeNode::Connection { id: "c2".into() },
    ];
    let p = payload_with(tree.clone(), &["c1", "c2"]);
    validate_tree(&p, &tree).unwrap();
}

#[test]
fn rejects_bad_trees() {
    let p = payload_with(vec![], &[]);
    // Unknown connection ref.
    assert!(validate_tree(&p, &[TreeNode::Connection { id: "ghost".into() }]).is_err());

    // Duplicate folder ids.
    let dup = vec![folder("f", vec![]), folder("f", vec![])];
    assert!(validate_tree(&p, &dup).is_err());

    // Connection stored but missing from the tree.
    let p2 = payload_with(vec![], &["c1"]);
    assert!(validate_tree(&p2, &[]).is_err());
}

#[test]
fn delete_folder_lifts_children() {
    let mut tree = vec![folder(
        "outer",
        vec![
            TreeNode::Connection { id: "c1".into() },
            folder("inner", vec![TreeNode::Connection { id: "c2".into() }]),
        ],
    )];
    remove_nodes(
        &mut tree,
        &|n| matches!(n, TreeNode::Folder { id, .. } if id == "outer"),
        true,
    );
    assert_eq!(tree.len(), 2);
    assert!(matches!(&tree[0], TreeNode::Connection { id } if id == "c1"));
    assert!(matches!(&tree[1], TreeNode::Folder { id, .. } if id == "inner"));
}

#[test]
fn insert_into_nested_folder() {
    let mut tree = vec![folder("a", vec![folder("b", vec![])])];
    insert_node(
        &mut tree,
        Some("b"),
        TreeNode::Connection { id: "c1".into() },
    )
    .unwrap();
    let found = find_folder_mut(&mut tree, "b").unwrap();
    assert_eq!(found.len(), 1);
    assert!(insert_node(
        &mut tree,
        Some("zzz"),
        TreeNode::Connection { id: "x".into() }
    )
    .is_err());
}
