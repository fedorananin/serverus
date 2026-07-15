use std::path::PathBuf;

use serde::Serialize;

use crate::workspace::FixturePaths;

#[derive(Debug, Serialize)]
pub struct FixtureManifest {
    pub paths: FixturePaths,
    pub ftp: FtpManifest,
    pub s3: S3Manifest,
    pub ssh: SshManifest,
    pub editor: EditorManifest,
}

impl FixtureManifest {
    pub fn new(
        paths: FixturePaths,
        ftp_port: u16,
        s3_port: u16,
        ssh: SshManifest,
        editor_executable: impl Into<PathBuf>,
    ) -> Self {
        Self {
            paths,
            ftp: FtpManifest {
                host: "127.0.0.1",
                port: ftp_port,
                username: "anonymous",
            },
            s3: S3Manifest {
                endpoint: format!("http://127.0.0.1:{s3_port}"),
                port: s3_port,
            },
            ssh,
            editor: EditorManifest {
                executable: editor_executable.into(),
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EditorManifest {
    pub executable: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct FtpManifest {
    pub host: &'static str,
    pub port: u16,
    pub username: &'static str,
}

#[derive(Debug, Serialize)]
pub struct S3Manifest {
    pub endpoint: String,
    pub port: u16,
}

#[derive(Clone, Debug, Serialize)]
pub struct SshManifest {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_path: Option<PathBuf>,
}

impl SshManifest {
    pub fn available(port: u16, username: impl Into<String>, key_path: impl Into<PathBuf>) -> Self {
        Self {
            available: true,
            host: Some("127.0.0.1"),
            port: Some(port),
            username: Some(username.into()),
            key_path: Some(key_path.into()),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            available: false,
            host: None,
            port: None,
            username: None,
            key_path: None,
        }
    }
}
