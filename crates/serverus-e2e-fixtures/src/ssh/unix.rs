use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::{bail, ensure, Context, Result};

use crate::manifest::SshManifest;
use crate::net::{reserve_local_port, wait_for_listener};
use crate::workspace::FixturePaths;

pub struct SshServer {
    child: Child,
    manifest: SshManifest,
}

impl SshServer {
    pub async fn start(paths: &FixturePaths) -> Result<Self> {
        let runtime_dir = paths.workspace_root.join(".ssh-fixture");
        fs::create_dir_all(&runtime_dir).context("create SSH fixture directory")?;
        let host_key = runtime_dir.join("host_ed25519");
        generate_key(&host_key)?;
        let client_key = runtime_dir.join("client_ed25519");
        generate_key(&client_key)?;
        let authorized_keys = runtime_dir.join("authorized_keys");
        fs::copy(client_key.with_extension("pub"), &authorized_keys)
            .context("create SSH authorized_keys")?;
        let username = current_username()?;
        let port = reserve_local_port()?;
        let config = runtime_dir.join("sshd_config");
        write_config(
            &config,
            port,
            &host_key,
            &authorized_keys,
            &username,
            &paths.ssh_root,
            &runtime_dir,
        )?;

        #[allow(clippy::zombie_processes)]
        let mut child = Command::new(find_sshd()?)
            .arg("-D")
            .arg("-e")
            .arg("-f")
            .arg(&config)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("spawn SSH fixture")?;

        if let Err(error) = wait_for_listener(port).await {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error).context("start SSH fixture");
        }

        Ok(Self {
            child,
            manifest: SshManifest::available(port, username, client_key),
        })
    }

    pub fn manifest(&self) -> &SshManifest {
        &self.manifest
    }
}

impl Drop for SshServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn generate_key(path: &Path) -> Result<()> {
    let status = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("run ssh-keygen for SSH fixture")?;
    ensure!(status.success(), "ssh-keygen failed for SSH fixture");
    Ok(())
}

fn find_sshd() -> Result<PathBuf> {
    for candidate in ["/usr/sbin/sshd", "/usr/bin/sshd", "/usr/local/sbin/sshd"] {
        if Path::new(candidate).is_file() {
            return Ok(PathBuf::from(candidate));
        }
    }
    bail!("sshd not found; install OpenSSH server")
}

fn find_sftp_server() -> &'static str {
    for candidate in [
        "/usr/libexec/sftp-server",
        "/usr/lib/openssh/sftp-server",
        "/usr/lib/ssh/sftp-server",
        "/usr/libexec/openssh/sftp-server",
    ] {
        if Path::new(candidate).is_file() {
            return candidate;
        }
    }
    "internal-sftp"
}

fn current_username() -> Result<String> {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .context("USER or LOGNAME is required for SSH fixture")
}

#[allow(clippy::too_many_arguments)]
fn write_config(
    path: &Path,
    port: u16,
    host_key: &Path,
    authorized_keys: &Path,
    username: &str,
    ssh_root: &Path,
    runtime_dir: &Path,
) -> Result<()> {
    let config = format!(
        r#"Port {port}
ListenAddress 127.0.0.1
HostKey {host_key}
PidFile {pid_file}
UsePAM no
StrictModes no
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
AuthorizedKeysFile {authorized_keys}
AllowUsers {username}
Subsystem sftp {sftp_server}
SetEnv HOME={ssh_root} ZDOTDIR={ssh_root}
LogLevel ERROR
"#,
        host_key = host_key.display(),
        pid_file = runtime_dir.join("sshd.pid").display(),
        authorized_keys = authorized_keys.display(),
        sftp_server = find_sftp_server(),
        ssh_root = ssh_root.display(),
    );
    fs::write(path, config).context("write SSH fixture config")
}
