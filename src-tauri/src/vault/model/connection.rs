use serde::{Deserialize, Serialize};
use specta::Type;

use super::Badge;

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
