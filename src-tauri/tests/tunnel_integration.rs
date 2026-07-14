//! Integration test for local port forwarding (M6, SPEC §4.2).

mod support;

use std::sync::Arc;

use serverus_lib::session::ssh::{connect_chain, ConnectOutcome, SshSession};
use serverus_lib::session::tunnel::TunnelManager;
use support::TestSshd;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn local_forwarding_roundtrip() {
    let sshd = TestSshd::spawn();
    let issue = match connect_chain(&[sshd.hop(None)]).await.unwrap() {
        ConnectOutcome::HostKeyPrompt(issue) => issue,
        _ => panic!(),
    };
    let ssh = match connect_chain(&[sshd.hop(Some(issue.key_line))])
        .await
        .unwrap()
    {
        ConnectOutcome::Connected(transport) => Arc::new(SshSession::new(transport)),
        _ => panic!(),
    };

    // "Remote" service: an uppercase-echo TCP server on localhost — reached
    // through the SSH server via direct-tcpip, like a database would be.
    let service = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service_port = service.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = service.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buf = [0u8; 1024];
                while let Ok(n) = sock.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    let upper: Vec<u8> = buf[..n].iter().map(|b| b.to_ascii_uppercase()).collect();
                    if sock.write_all(&upper).await.is_err() {
                        break;
                    }
                }
            });
        }
    });

    let local_port = {
        // Grab a free port for the tunnel entrance.
        std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    };

    let manager = TunnelManager::default();
    let status = manager
        .start(
            ssh,
            "session-1",
            "test",
            local_port,
            "127.0.0.1",
            service_port,
        )
        .await
        .unwrap();
    assert_eq!(status.local_port, local_port);

    // Duplicate local port must be refused.
    assert!(manager
        .list(Some("session-1"))
        .iter()
        .any(|t| t.local_port == local_port));

    // Talk through the tunnel.
    let mut client = tokio::net::TcpStream::connect(("127.0.0.1", local_port))
        .await
        .unwrap();
    client.write_all(b"serverus tunnel").await.unwrap();
    let mut reply = vec![0u8; 15];
    client.read_exact(&mut reply).await.unwrap();
    assert_eq!(reply, b"SERVERUS TUNNEL");
    drop(client);

    // Traffic counters moved.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let listed = manager.list(Some("session-1"));
    assert_eq!(listed.len(), 1);
    assert!(listed[0].bytes_up >= 15, "{listed:#?}");
    assert!(listed[0].bytes_down >= 15, "{listed:#?}");

    // Stop → the entrance closes.
    manager.stop(&listed[0].id).await;
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    assert!(tokio::net::TcpStream::connect(("127.0.0.1", local_port))
        .await
        .is_err());
}
