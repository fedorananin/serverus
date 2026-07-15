use std::io::{self, Write};

use anyhow::{Context, Result};
use serverus_e2e_fixtures::editor::rewrite_scenario_file;
use serverus_e2e_fixtures::ftp::FtpServer;
use serverus_e2e_fixtures::manifest::FixtureManifest;
use serverus_e2e_fixtures::s3::S3Server;
use serverus_e2e_fixtures::ssh::SshServer;
use serverus_e2e_fixtures::workspace::FixtureWorkspace;

#[tokio::main]
async fn main() -> Result<()> {
    if let Some(path) = std::env::args_os().nth(1) {
        return rewrite_scenario_file(std::path::Path::new(&path));
    }

    let editor_executable = std::env::current_exe().context("locate fixture editor executable")?;
    let workspace = FixtureWorkspace::create()?;
    let ftp = FtpServer::start(&workspace.paths().ftp_root).await?;
    let s3 = S3Server::start(&workspace.paths().s3_root).await?;
    let ssh = SshServer::start(workspace.paths()).await?;
    let manifest = FixtureManifest::new(
        workspace.paths().clone(),
        ftp.port(),
        s3.port(),
        ssh.manifest().clone(),
        editor_executable,
    );

    write_manifest(&manifest)?;
    wait_for_stdin_eof().await?;
    Ok(())
}

fn write_manifest(manifest: &FixtureManifest) -> Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, manifest).context("serialize fixture manifest")?;
    stdout.write_all(b"\n").context("write fixture manifest")?;
    stdout.flush().context("flush fixture manifest")?;
    Ok(())
}

async fn wait_for_stdin_eof() -> Result<()> {
    tokio::task::spawn_blocking(|| {
        let stdin = io::stdin();
        let mut stdin = stdin.lock();
        io::copy(&mut stdin, &mut io::sink())
    })
    .await
    .context("join fixture stdin watcher")??;
    Ok(())
}
