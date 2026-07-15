//! Vault payload data model (SPEC §3, §8).
//!
//! Everything here is serialized into the encrypted vault payload. Types that
//! cross the IPC boundary have redacted "public" counterparts further below —
//! secrets (passwords, passphrases, inline keys) never leave the backend.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use specta::Type;

pub const PAYLOAD_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Tree & badges
// ---------------------------------------------------------------------------

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
/// references into [`VaultPayload::connections`].
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

// ---------------------------------------------------------------------------
// Connections
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    /// SSH covers terminal + SFTP + tunnels.
    Ssh,
    Ftp,
    /// S3-compatible object storage (AWS S3, DigitalOcean Spaces, R2, …).
    S3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    Password,
    Key,
    Agent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub method: AuthMethod,
    pub username: String,
    /// Secret — never serialized to the frontend.
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub key_path: Option<String>,
    /// Secret — inline private key PEM.
    #[serde(default)]
    pub key_inline: Option<String>,
    /// Secret.
    #[serde(default)]
    pub key_passphrase: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FtpTlsMode {
    None,
    /// Explicit FTPS (AUTH TLS). When set, plaintext fallback is forbidden.
    Explicit,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct FtpOptions {
    pub tls: FtpTlsMode,
    pub passive: bool,
}

impl Default for FtpOptions {
    fn default() -> Self {
        FtpOptions {
            tls: FtpTlsMode::None,
            passive: true,
        }
    }
}

/// Default ACL applied to objects uploaded to an S3 connection (SPEC §4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum S3UploadAcl {
    Private,
    PublicRead,
    /// The UI asks per upload batch.
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct S3Options {
    /// Signing region. Providers other than AWS often accept any value;
    /// empty falls back to `us-east-1`.
    #[serde(default)]
    pub region: Option<String>,
    /// Lock the connection to one bucket. Empty = the root of the panel
    /// lists all buckets of the account as folders.
    #[serde(default)]
    pub bucket: Option<String>,
    /// Path-style addressing (`https://endpoint/bucket/key`) for MinIO and
    /// other self-hosted gateways; default is virtual-host style.
    #[serde(default)]
    pub path_style: bool,
    /// Base for "Copy public URL" (CDN endpoint / custom domain). Empty =
    /// build the URL from the storage endpoint.
    #[serde(default)]
    pub public_base_url: Option<String>,
    pub upload_acl: S3UploadAcl,
}

impl Default for S3Options {
    fn default() -> Self {
        S3Options {
            region: None,
            bucket: None,
            path_style: false,
            public_base_url: None,
            upload_acl: S3UploadAcl::Private,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum TunnelKind {
    /// `localhost:<local_port>` → SSH → `<remote_host>:<remote_port>`.
    Local,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TunnelConfig {
    pub name: String,
    pub kind: TunnelKind,
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    #[serde(default)]
    pub autostart: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    pub name: String,
    #[serde(default)]
    pub badge: Option<Badge>,
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub auth: AuthConfig,
    /// Reference to another connection used as a bastion; chains recursively.
    #[serde(default)]
    pub jump_host: Option<String>,
    #[serde(default)]
    pub ftp: Option<FtpOptions>,
    #[serde(default)]
    pub s3: Option<S3Options>,
    #[serde(default)]
    pub remote_dir: Option<String>,
    #[serde(default)]
    pub local_dir: Option<String>,
    #[serde(default)]
    pub tunnels: Vec<TunnelConfig>,
    /// SFTP-only servers: the account has no shell, so the UI must not offer
    /// a terminal (and never opens a shell channel).
    #[serde(default)]
    pub disable_terminal: bool,
    #[serde(default)]
    pub notes: String,
}

// ---------------------------------------------------------------------------
// Settings (SPEC §8)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct SecuritySettings {
    /// 0 = never.
    pub auto_lock_minutes: u32,
    pub lock_on_sleep: bool,
    pub touch_id: bool,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        SecuritySettings {
            auto_lock_minutes: 15,
            lock_on_sleep: true,
            touch_id: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    Ask,
    Overwrite,
    Skip,
    Rename,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TransferSettings {
    pub max_parallel_per_server: u32,
    pub conflict_policy: ConflictPolicy,
    pub preserve_mtime: bool,
    pub tar_acceleration: bool,
}

impl Default for TransferSettings {
    fn default() -> Self {
        TransferSettings {
            max_parallel_per_server: 5,
            conflict_policy: ConflictPolicy::Ask,
            preserve_mtime: true,
            tar_acceleration: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct EditorSettings {
    /// When false, `custom_app` names the editor application to use.
    pub use_system_default: bool,
    pub custom_app: Option<String>,
}

impl Default for EditorSettings {
    fn default() -> Self {
        EditorSettings {
            use_system_default: true,
            custom_app: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct TerminalSettings {
    pub font_family: String,
    pub font_size: u16,
    pub scrollback: u32,
    /// Copy the selection to the clipboard automatically. Off by default —
    /// it silently clobbers whatever the user copied elsewhere.
    #[serde(default)]
    pub copy_on_select: bool,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        // xterm.js takes a CSS font-family list, so fallbacks are free.
        let font_family = if cfg!(target_os = "macos") {
            "SF Mono"
        } else if cfg!(target_os = "windows") {
            "Cascadia Mono, Consolas, monospace"
        } else {
            "DejaVu Sans Mono, monospace"
        };
        TerminalSettings {
            font_family: font_family.into(),
            font_size: 13,
            scrollback: 10_000,
            copy_on_select: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SizeFormat {
    /// Powers of 1000 (KB).
    Kb,
    /// Powers of 1024 (KiB).
    Kib,
}

/// Sidebar width bounds, in CSS pixels. The floor keeps connection names
/// readable; the ceiling keeps the file panes and terminal usable at the
/// minimum window width (940, see `tauri.conf.json`).
pub const SIDEBAR_WIDTH_MIN: u16 = 200;
pub const SIDEBAR_WIDTH_MAX: u16 = 380;
pub const SIDEBAR_WIDTH_DEFAULT: u16 = 230;

fn default_sidebar_width() -> u16 {
    SIDEBAR_WIDTH_DEFAULT
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PanelSettings {
    pub show_hidden: bool,
    pub size_format: SizeFormat,
    pub default_local_dir: Option<String>,
    /// Sidebar width in CSS pixels, always within
    /// [`SIDEBAR_WIDTH_MIN`]..=[`SIDEBAR_WIDTH_MAX`].
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: u16,
}

impl Default for PanelSettings {
    fn default() -> Self {
        PanelSettings {
            show_hidden: false,
            size_format: SizeFormat::Kib,
            default_local_dir: None,
            sidebar_width: SIDEBAR_WIDTH_DEFAULT,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct Settings {
    pub security: SecuritySettings,
    pub transfers: TransferSettings,
    pub editor: EditorSettings,
    pub terminal: TerminalSettings,
    pub panels: PanelSettings,
}

impl Settings {
    /// Force values that would wedge the UI back into range. The frontend
    /// clamps while dragging, but a hand-edited vault must not be able to
    /// leave the sidebar unusably wide or narrow.
    pub fn clamp(&mut self) {
        self.panels.sidebar_width = self
            .panels
            .sidebar_width
            .clamp(SIDEBAR_WIDTH_MIN, SIDEBAR_WIDTH_MAX);
    }
}

// ---------------------------------------------------------------------------
// Payload root
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Redacted DTOs crossing the IPC boundary
// ---------------------------------------------------------------------------

/// Auth config with secrets replaced by presence flags.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PublicAuth {
    pub method: AuthMethod,
    pub username: String,
    pub key_path: Option<String>,
    pub has_password: bool,
    pub has_key_inline: bool,
    pub has_key_passphrase: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PublicConnection {
    pub id: String,
    pub name: String,
    pub badge: Option<Badge>,
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub auth: PublicAuth,
    pub jump_host: Option<String>,
    pub ftp: Option<FtpOptions>,
    pub s3: Option<S3Options>,
    pub remote_dir: Option<String>,
    pub local_dir: Option<String>,
    pub tunnels: Vec<TunnelConfig>,
    pub disable_terminal: bool,
    pub notes: String,
}

/// The whole vault as the UI sees it — no secrets.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct PublicVault {
    pub tree: Vec<TreeNode>,
    pub connections: HashMap<String, PublicConnection>,
    pub known_hosts: HashMap<String, String>,
    pub settings: Settings,
}

impl Connection {
    pub fn to_public(&self, id: &str) -> PublicConnection {
        PublicConnection {
            id: id.to_string(),
            name: self.name.clone(),
            badge: self.badge.clone(),
            protocol: self.protocol,
            host: self.host.clone(),
            port: self.port,
            auth: PublicAuth {
                method: self.auth.method,
                username: self.auth.username.clone(),
                key_path: self.auth.key_path.clone(),
                has_password: self.auth.password.is_some(),
                has_key_inline: self.auth.key_inline.is_some(),
                has_key_passphrase: self.auth.key_passphrase.is_some(),
            },
            jump_host: self.jump_host.clone(),
            ftp: self.ftp.clone(),
            s3: self.s3.clone(),
            remote_dir: self.remote_dir.clone(),
            local_dir: self.local_dir.clone(),
            tunnels: self.tunnels.clone(),
            disable_terminal: self.disable_terminal,
            notes: self.notes.clone(),
        }
    }
}

impl VaultPayload {
    pub fn to_public(&self) -> PublicVault {
        PublicVault {
            tree: self.tree.clone(),
            connections: self
                .connections
                .iter()
                .map(|(id, c)| (id.clone(), c.to_public(id)))
                .collect(),
            known_hosts: self.known_hosts.clone(),
            settings: self.settings.clone(),
        }
    }
}

/// Input for creating/updating a connection from the UI. Secret fields are
/// "tri-state": `None` = keep the stored value, `Some("")` = clear,
/// `Some(v)` = replace.
#[derive(Debug, Clone, Deserialize, Type)]
pub struct ConnectionInput {
    pub name: String,
    pub badge: Option<Badge>,
    pub protocol: Protocol,
    pub host: String,
    pub port: u16,
    pub auth_method: AuthMethod,
    pub username: String,
    pub password: Option<String>,
    pub key_path: Option<String>,
    pub key_inline: Option<String>,
    pub key_passphrase: Option<String>,
    pub jump_host: Option<String>,
    pub ftp: Option<FtpOptions>,
    pub s3: Option<S3Options>,
    pub remote_dir: Option<String>,
    pub local_dir: Option<String>,
    pub tunnels: Vec<TunnelConfig>,
    #[serde(default)]
    pub disable_terminal: bool,
    pub notes: String,
}

fn merge_secret(new: Option<String>, old: Option<String>) -> Option<String> {
    match new {
        None => old,
        Some(s) if s.is_empty() => None,
        Some(s) => Some(s),
    }
}

impl ConnectionInput {
    /// Merge this input over an existing connection (or a blank one).
    pub fn into_connection(self, existing: Option<&Connection>) -> Connection {
        let old = existing.map(|c| c.auth.clone());
        Connection {
            name: self.name,
            badge: self.badge,
            protocol: self.protocol,
            host: self.host,
            port: self.port,
            auth: AuthConfig {
                method: self.auth_method,
                username: self.username,
                password: merge_secret(
                    self.password,
                    old.as_ref().and_then(|a| a.password.clone()),
                ),
                key_path: self.key_path,
                key_inline: merge_secret(
                    self.key_inline,
                    old.as_ref().and_then(|a| a.key_inline.clone()),
                ),
                key_passphrase: merge_secret(
                    self.key_passphrase,
                    old.as_ref().and_then(|a| a.key_passphrase.clone()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidebar_width_defaults_when_absent() {
        // Vaults written before the sidebar was resizable have no field.
        let panels: PanelSettings =
            serde_json::from_str(r#"{"show_hidden":false,"size_format":"kib","default_local_dir":null}"#)
                .unwrap();
        assert_eq!(panels.sidebar_width, SIDEBAR_WIDTH_DEFAULT);
    }

    #[test]
    fn folders_from_older_vaults_are_expanded() {
        // Vaults written before folders remembered their disclosure state.
        let node: TreeNode =
            serde_json::from_str(r#"{"type":"folder","id":"f1","name":"Prod"}"#).unwrap();
        assert!(matches!(node, TreeNode::Folder { collapsed: false, .. }));
    }

    #[test]
    fn clamp_forces_sidebar_width_into_range() {
        let mut s = Settings::default();

        s.panels.sidebar_width = 5000;
        s.clamp();
        assert_eq!(s.panels.sidebar_width, SIDEBAR_WIDTH_MAX);

        s.panels.sidebar_width = 0;
        s.clamp();
        assert_eq!(s.panels.sidebar_width, SIDEBAR_WIDTH_MIN);

        s.panels.sidebar_width = 300;
        s.clamp();
        assert_eq!(s.panels.sidebar_width, 300);
    }
}
