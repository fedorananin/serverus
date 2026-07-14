//! Integration tests for the SSH core (M2) against a real unprivileged sshd.

mod support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use serverus_lib::session::ssh::{connect_chain, ConnectOutcome};
use support::TestSshd;

struct TrackingProxy {
    port: u16,
    active: Arc<AtomicUsize>,
    changed: Arc<tokio::sync::Notify>,
    listener: tokio::task::JoinHandle<()>,
}

impl TrackingProxy {
    async fn spawn(backend_port: u16) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let active = Arc::new(AtomicUsize::new(0));
        let changed = Arc::new(tokio::sync::Notify::new());
        let task_active = active.clone();
        let task_changed = changed.clone();
        let task = tokio::spawn(async move {
            while let Ok((mut incoming, _)) = listener.accept().await {
                let active = task_active.clone();
                let changed = task_changed.clone();
                tokio::spawn(async move {
                    let Ok(mut backend) =
                        tokio::net::TcpStream::connect(("127.0.0.1", backend_port)).await
                    else {
                        return;
                    };
                    active.fetch_add(1, Ordering::SeqCst);
                    changed.notify_waiters();
                    let _ = tokio::io::copy_bidirectional(&mut incoming, &mut backend).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    changed.notify_waiters();
                });
            }
        });
        Self {
            port,
            active,
            changed,
            listener: task,
        }
    }

    async fn wait_active(&self, expected: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let changed = self.changed.notified();
                if self.active.load(Ordering::SeqCst) == expected {
                    return;
                }
                changed.await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("proxy never reached {expected} active connections"));
    }
}

impl Drop for TrackingProxy {
    fn drop(&mut self) {
        self.listener.abort();
    }
}

async fn known_key(sshd: &TestSshd) -> String {
    match connect_chain(&[sshd.hop(None)]).await.unwrap() {
        ConnectOutcome::HostKeyPrompt(issue) => issue.key_line,
        ConnectOutcome::Connected(_) => panic!("expected host key prompt"),
    }
}

/// First connection: no stored key → host key prompt with a fingerprint;
/// accepting the offered line lets the next attempt through (SPEC §4.1).
#[tokio::test]
async fn host_key_prompt_then_connect() {
    let sshd = TestSshd::spawn();

    let outcome = connect_chain(&[sshd.hop(None)]).await.unwrap();
    let issue = match outcome {
        ConnectOutcome::HostKeyPrompt(issue) => issue,
        ConnectOutcome::Connected(_) => panic!("must prompt on unknown host key"),
    };
    assert!(!issue.changed);
    assert!(issue.fingerprint.starts_with("SHA256:"));
    assert!(issue.key_line.starts_with("ssh-ed25519 "));

    // Accept: reconnect with the offered key line.
    let outcome = connect_chain(&[sshd.hop(Some(issue.key_line.clone()))])
        .await
        .unwrap();
    assert!(matches!(outcome, ConnectOutcome::Connected(_)));
}

#[tokio::test]
async fn disconnect_waits_until_the_server_transport_is_closed() {
    let sshd = TestSshd::spawn();
    let known_key = known_key(&sshd).await;
    let proxy = TrackingProxy::spawn(sshd.port).await;
    let mut hop = sshd.hop(Some(known_key));
    hop.port = proxy.port;
    let transport = match connect_chain(&[hop]).await.unwrap() {
        ConnectOutcome::Connected(transport) => transport,
        ConnectOutcome::HostKeyPrompt(_) => panic!("known key was rejected"),
    };
    proxy.wait_active(1).await;

    transport.disconnect_and_wait().await.unwrap();

    proxy.wait_active(0).await;
}

#[tokio::test]
async fn jump_chain_retains_and_closes_every_hop_transport() {
    let bastion = TestSshd::spawn();
    let target = TestSshd::spawn();
    let bastion_key = known_key(&bastion).await;
    let target_key = known_key(&target).await;
    let bastion_proxy = TrackingProxy::spawn(bastion.port).await;
    let target_proxy = TrackingProxy::spawn(target.port).await;
    let mut bastion_hop = bastion.hop(Some(bastion_key));
    bastion_hop.port = bastion_proxy.port;
    let mut target_hop = target.hop(Some(target_key));
    target_hop.port = target_proxy.port;
    let transport = match connect_chain(&[bastion_hop, target_hop]).await.unwrap() {
        ConnectOutcome::Connected(transport) => transport,
        ConnectOutcome::HostKeyPrompt(_) => panic!("known jump-host key was rejected"),
    };
    bastion_proxy.wait_active(1).await;
    target_proxy.wait_active(1).await;

    transport.disconnect_and_wait().await.unwrap();

    target_proxy.wait_active(0).await;
    bastion_proxy.wait_active(0).await;
}

/// A stored-but-different key must surface as `changed` (MITM warning).
#[tokio::test]
async fn changed_host_key_flagged() {
    let sshd = TestSshd::spawn();
    let bogus = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let outcome = connect_chain(&[sshd.hop(Some(bogus.into()))])
        .await
        .unwrap();
    match outcome {
        ConnectOutcome::HostKeyPrompt(issue) => assert!(issue.changed),
        ConnectOutcome::Connected(_) => panic!("must not connect with a mismatched key"),
    }
}

/// Wrong key file → auth error, not a hang or panic.
#[tokio::test]
async fn bad_key_fails_auth() {
    let sshd = TestSshd::spawn();

    // Learn the host key first.
    let issue = match connect_chain(&[sshd.hop(None)]).await.unwrap() {
        ConnectOutcome::HostKeyPrompt(issue) => issue,
        _ => panic!(),
    };

    // Generate an unrelated key and try to use it.
    let dir = tempfile::tempdir().unwrap();
    let other = dir.path().join("other");
    std::process::Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&other)
        .status()
        .unwrap();

    let mut hop = sshd.hop(Some(issue.key_line.clone()));
    hop.auth.key_path = Some(other.to_string_lossy().into_owned());
    let msg = match connect_chain(&[hop]).await {
        Err(e) => e.to_string(),
        Ok(_) => panic!("auth must fail with an unrelated key"),
    };
    assert!(msg.contains("authentication failed"), "got: {msg}");
}

/// Full terminal roundtrip: PTY + shell, echo a marker, read it back.
#[tokio::test]
async fn shell_echo_roundtrip() {
    let sshd = TestSshd::spawn();
    let issue = match connect_chain(&[sshd.hop(None)]).await.unwrap() {
        ConnectOutcome::HostKeyPrompt(issue) => issue,
        _ => panic!(),
    };
    let handle = match connect_chain(&[sshd.hop(Some(issue.key_line))])
        .await
        .unwrap()
    {
        ConnectOutcome::Connected(handle) => handle,
        _ => panic!(),
    };

    let channel = handle.channel_open_session().await.unwrap();
    channel
        .request_pty(true, "xterm-256color", 80, 24, 0, 0, &[])
        .await
        .unwrap();
    channel.request_shell(true).await.unwrap();

    let (mut read, write) = channel.split();
    write.data(&b"echo serverus-$((6*7))\n"[..]).await.unwrap();

    let mut collected = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let msg = tokio::time::timeout_at(deadline, read.wait())
            .await
            .expect("shell answered in time");
        match msg {
            Some(russh::ChannelMsg::Data { data }) => {
                collected.push_str(&String::from_utf8_lossy(&data));
                if collected.contains("serverus-42") {
                    break;
                }
            }
            Some(_) => {}
            None => panic!("channel closed before marker; output: {collected}"),
        }
    }

    handle.disconnect_and_wait().await.unwrap();
}
