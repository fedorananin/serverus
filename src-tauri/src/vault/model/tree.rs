use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BadgeKind {
    Emoji,
    Color,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct Badge {
    pub kind: BadgeKind,
    /// Emoji character or hex color like `#e5484d`.
    pub value: String,
}

/// Sidebar tree node. Folders nest arbitrarily; connection nodes are
/// references into [`super::VaultPayload::connections`].
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TreeNode {
    Folder {
        id: String,
        name: String,
        #[serde(default)]
        badge: Option<Badge>,
        #[serde(default)]
        children: Vec<TreeNode>,
        /// Sidebar disclosure state. Expanded is the default, so `false` is
        /// also the right value for vaults written before this existed.
        #[serde(default)]
        collapsed: bool,
    },
    Connection {
        id: String,
    },
}
