use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{Connection, Settings, TreeNode};

pub const PAYLOAD_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultPayload {
    pub version: u32,
    #[serde(default)]
    pub tree: Vec<TreeNode>,
    #[serde(default)]
    pub connections: HashMap<String, Connection>,
    /// `host:port` → `algo base64-public-key` accepted by the user.
    #[serde(default)]
    pub known_hosts: HashMap<String, String>,
    #[serde(default)]
    pub settings: Settings,
}

impl Default for VaultPayload {
    fn default() -> Self {
        VaultPayload {
            version: PAYLOAD_VERSION,
            tree: Vec::new(),
            connections: HashMap::new(),
            known_hosts: HashMap::new(),
            settings: Settings::default(),
        }
    }
}
