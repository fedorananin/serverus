use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct FixturePaths {
    pub workspace_root: PathBuf,
    pub app_config_dir: PathBuf,
    pub vault_dir: PathBuf,
    pub local_source: PathBuf,
    pub local_download: PathBuf,
    pub ftp_root: PathBuf,
    pub s3_root: PathBuf,
    pub ssh_root: PathBuf,
}

pub struct FixtureWorkspace {
    _root: tempfile::TempDir,
    paths: FixturePaths,
}

impl FixtureWorkspace {
    pub fn create() -> Result<Self> {
        let root = tempfile::Builder::new()
            .prefix("serverus-e2e-")
            .tempdir()
            .context("create fixture workspace")?;
        let workspace_root = root.path().to_path_buf();
        let paths = FixturePaths {
            app_config_dir: workspace_root.join("app-config"),
            vault_dir: workspace_root.join("vaults"),
            local_source: workspace_root.join("local-source"),
            local_download: workspace_root.join("local-download"),
            ftp_root: workspace_root.join("ftp-root"),
            s3_root: workspace_root.join("s3-root"),
            ssh_root: workspace_root.join("ssh-root"),
            workspace_root,
        };

        for path in [
            &paths.app_config_dir,
            &paths.vault_dir,
            &paths.local_download,
            &paths.ftp_root,
            &paths.s3_root,
            &paths.ssh_root,
        ] {
            fs::create_dir_all(path)
                .with_context(|| format!("create fixture directory {}", path.display()))?;
        }
        seed_local_source(&paths.local_source)?;
        for root in [&paths.ftp_root, &paths.s3_root, &paths.ssh_root] {
            seed_remote_tree(root)?;
        }
        seed_session_cleanup(&paths.ssh_root)?;
        seed_transfer_resilience(&paths.local_source, &paths.ftp_root)?;
        seed_remote_edit(&paths.ftp_root)?;

        Ok(Self { _root: root, paths })
    }

    pub fn paths(&self) -> &FixturePaths {
        &self.paths
    }
}

fn seed_local_source(root: &Path) -> Result<()> {
    let site = root.join("site");
    fs::create_dir_all(site.join("assets/styles"))?;
    fs::create_dir_all(site.join("assets/images"))?;
    fs::create_dir_all(site.join("empty"))?;
    fs::write(site.join("index.html"), "<main>Serverus E2E</main>\n")?;
    fs::write(
        site.join("assets/styles/app.css"),
        "body { color: #fff; }\n",
    )?;
    fs::write(site.join("assets/images/logo.txt"), "S>\n")?;
    Ok(())
}

fn seed_remote_tree(root: &Path) -> Result<()> {
    let site = root.join("serverus-e2e/site");
    fs::create_dir_all(site.join("nested"))?;
    fs::create_dir_all(site.join("empty"))?;
    fs::write(site.join("index.html"), "<main>remote fixture</main>\n")?;
    fs::write(site.join("nested/readme.txt"), "nested fixture\n")?;
    Ok(())
}

fn seed_session_cleanup(ssh_root: &Path) -> Result<()> {
    let path = ssh_root.join("serverus-e2e/site/cleanup-slow.bin");
    fs::File::create(path)?.set_len(4 * 1024 * 1024)?;
    Ok(())
}

fn seed_transfer_resilience(local_root: &Path, ftp_root: &Path) -> Result<()> {
    let local = local_root.join("conflicts");
    let remote = ftp_root.join("conflicts");
    for batch in ["batch", "batch-skip", "batch-rename"] {
        fs::create_dir_all(local.join(batch))?;
        fs::create_dir_all(remote.join(batch))?;
    }

    for (name, local_content, remote_content) in [
        ("overwrite.txt", "local overwrite\n", "remote original\n"),
        ("skip.txt", "local skip\n", "remote skip\n"),
        ("rename.txt", "local rename\n", "remote rename\n"),
        (
            "after-batch.txt",
            "local after batch\n",
            "remote after batch\n",
        ),
    ] {
        fs::write(local.join(name), local_content)?;
        fs::write(remote.join(name), remote_content)?;
    }
    for (name, local_content, remote_content) in [
        ("batch-a.txt", "local batch a\n", "remote batch a\n"),
        ("batch-b.txt", "local batch b\n", "remote batch b\n"),
    ] {
        fs::write(local.join("batch").join(name), local_content)?;
        fs::write(remote.join("batch").join(name), remote_content)?;
    }
    for (batch, action) in [("batch-skip", "skip"), ("batch-rename", "rename")] {
        for suffix in ["a", "b"] {
            let name = format!("batch-{suffix}.txt");
            fs::write(
                local.join(batch).join(&name),
                format!("local {action} batch {suffix}\n"),
            )?;
            fs::write(
                remote.join(batch).join(&name),
                format!("remote {action} batch {suffix}\n"),
            )?;
        }
    }

    let resume: Vec<u8> = (0..524_288).map(|index| (index % 251) as u8).collect();
    fs::write(remote.join("resume.bin"), resume)?;
    Ok(())
}

fn seed_remote_edit(ftp_root: &Path) -> Result<()> {
    fs::write(
        ftp_root.join("edit-success.txt"),
        "remote success original\n",
    )?;
    fs::write(
        ftp_root.join("edit-failure.txt"),
        "remote failure original\n",
    )?;
    Ok(())
}
