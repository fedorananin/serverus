//! Integration tests for the SSH core (M2) against a real unprivileged sshd.

mod support;

use serverus_lib::session::ssh::{connect_chain, ConnectOutcome};
use support::TestSshd;

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
    let isolated_home = sshd.dir.path().to_string_lossy().into_owned();
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
    write
        .data(&b"printf '\nSERVERUS_HOME=%s\nSERVERUS_MARKER=%s\n' \"$HOME\" \"$((6*7))\"\n"[..])
        .await
        .unwrap();

    let mut collected = String::new();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let msg = tokio::time::timeout_at(deadline, read.wait())
            .await
            .unwrap_or_else(|_| panic!("shell answered in time; output: {collected:?}"));
        match msg {
            Some(russh::ChannelMsg::Data { data }) => {
                collected.push_str(&String::from_utf8_lossy(&data));
                if collected.contains("SERVERUS_MARKER=42") {
                    break;
                }
            }
            Some(_) => {}
            None => panic!("channel closed before marker; output: {collected}"),
        }
    }
    assert!(
        collected.contains(&format!("SERVERUS_HOME={isolated_home}")),
        "shell inherited host HOME instead of the isolated fixture: {collected:?}"
    );

    let _ = handle
        .disconnect(russh::Disconnect::ByApplication, "", "en")
        .await;
}
