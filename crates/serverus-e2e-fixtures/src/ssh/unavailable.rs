use anyhow::Result;

use crate::manifest::SshManifest;
use crate::workspace::FixturePaths;

pub struct SshServer {
    manifest: SshManifest,
}

impl SshServer {
    pub async fn start(_paths: &FixturePaths) -> Result<Self> {
        Ok(Self {
            manifest: SshManifest::unavailable(),
        })
    }

    pub fn manifest(&self) -> &SshManifest {
        &self.manifest
    }
}
