//! Test fixtures: a real OpenSSH server (`sshd`) running unprivileged on a
//! random port. Replaces the docker-based openssh-server container from the
//! original plan — zero external dependencies, works locally and in CI.

use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

// Not every test binary uses every field of the shared fixture.
#[allow(dead_code)]
pub struct TestSshd {
    child: Child,
    pub port: u16,
    /// Scratch directory (also holds keys/config) used as the remote side.
    pub dir: tempfile::TempDir,
    pub user: String,
    pub key_path: PathBuf,
}

impl Drop for TestSshd {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn ssh_keygen(path: &Path, kind: &str) {
    let status = Command::new("ssh-keygen")
        .args(["-q", "-t", kind, "-N", "", "-f"])
        .arg(path)
        .status()
        .expect("ssh-keygen runs");
    assert!(status.success(), "ssh-keygen failed");
}

fn find_sshd() -> PathBuf {
    for candidate in ["/usr/sbin/sshd", "/usr/bin/sshd", "/usr/local/sbin/sshd"] {
        if Path::new(candidate).exists() {
            return PathBuf::from(candidate);
        }
    }
    panic!("sshd not found — install openssh-server");
}

fn find_sftp_server() -> &'static str {
    for candidate in [
        "/usr/libexec/sftp-server",         // macOS
        "/usr/lib/openssh/sftp-server",     // Debian/Ubuntu
        "/usr/lib/ssh/sftp-server",         // Arch
        "/usr/libexec/openssh/sftp-server", // Fedora
    ] {
        if Path::new(candidate).exists() {
            return candidate;
        }
    }
    "internal-sftp"
}

impl TestSshd {
    /// Start sshd on a random port. Authentication: publickey as the current
    /// user with a fresh throwaway key.
    pub fn spawn() -> TestSshd {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path();

        let host_key = base.join("host_ed25519");
        ssh_keygen(&host_key, "ed25519");
        let client_key = base.join("client_ed25519");
        ssh_keygen(&client_key, "ed25519");

        let authorized = base.join("authorized_keys");
        fs::copy(base.join("client_ed25519.pub"), &authorized).unwrap();

        let user = whoami();
        let config = base.join("sshd_config");
        // A port picked by `free_port` can be taken again before sshd binds
        // it (another test's outgoing connection draws from the same
        // ephemeral range); sshd then exits, and we retry on a fresh port.
        for _ in 0..5 {
            let port = free_port();
            write_config(&config, port, &host_key, &authorized, &user, base);
            // Reaped in Drop (kill + wait) or below when it exits early.
            #[allow(clippy::zombie_processes)]
            let mut child = Command::new(find_sshd())
                .arg("-D")
                .arg("-e")
                .arg("-f")
                .arg(&config)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("sshd starts");
            if wait_for_banner(&mut child, port) {
                return TestSshd {
                    child,
                    port,
                    dir,
                    user,
                    key_path: client_key,
                };
            }
            let _ = child.kill();
            let _ = child.wait();
        }
        panic!("sshd did not start listening");
    }

    /// A Hop pointing at this server with key auth.
    pub fn hop(&self, known_host_line: Option<String>) -> serverus_lib::session::ssh::Hop {
        serverus_lib::session::ssh::Hop {
            host: "127.0.0.1".into(),
            port: self.port,
            auth: serverus_lib::session::ssh::HopAuth {
                username: self.user.clone(),
                method: serverus_lib::vault::model::AuthMethod::Key,
                password: None,
                key_path: Some(self.key_path.to_string_lossy().into_owned()),
                key_inline: None,
                key_passphrase: None,
            },
            known_host_line,
        }
    }
}

fn whoami() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .expect("USER env var")
}

/// Wait until sshd on `port` greets with its `SSH-` banner. A bare TCP
/// connect is not proof: on Linux, connecting to a free port in the
/// ephemeral range can pick that same port locally and connect to itself,
/// which "succeeds" with nobody listening. Returns false once sshd exits.
fn wait_for_banner(child: &mut Child, port: u16) -> bool {
    use std::io::Read;

    for _ in 0..100 {
        if !matches!(child.try_wait(), Ok(None)) {
            return false;
        }
        if let Ok(mut stream) = std::net::TcpStream::connect(("127.0.0.1", port)) {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut banner = [0u8; 4];
            if stream.read_exact(&mut banner).is_ok() && &banner == b"SSH-" {
                return true;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn write_config(
    config: &Path,
    port: u16,
    host_key: &Path,
    authorized: &Path,
    user: &str,
    base: &Path,
) {
    fs::write(
        config,
        format!(
            r#"
Port {port}
ListenAddress 127.0.0.1
HostKey {host_key}
PidFile none
UsePAM no
StrictModes no
PasswordAuthentication no
KbdInteractiveAuthentication no
PubkeyAuthentication yes
AuthorizedKeysFile {authorized}
AllowUsers {user}
Subsystem sftp {sftp}
AcceptEnv LANG LC_*
# Interactive-shell tests must not source the host user's dotfiles. Those can
# perform arbitrary slow or stateful setup and make the SSH transport fixture
# nondeterministic.
SetEnv HOME={base} ZDOTDIR={base}
LogLevel ERROR
"#,
            port = port,
            host_key = host_key.display(),
            authorized = authorized.display(),
            user = user,
            sftp = find_sftp_server(),
            base = base.display(),
        ),
    )
    .unwrap();
}
