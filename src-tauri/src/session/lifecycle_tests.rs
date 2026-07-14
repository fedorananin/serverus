use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::AsyncReadExt;

use super::lifecycle::SessionLifecycle;
use super::{SessionEntry, SessionManager, TerminalEntry};
use crate::error::AppError;
use crate::session::s3::{S3Config, S3Fs};
use crate::vault::model::{Protocol, S3UploadAcl};

async fn assert_late_registration_is_rejected() {
    let manager = Arc::new(SessionManager::default());
    let entry = Arc::new(SessionEntry {
        id: "session".into(),
        connection_id: "connection".into(),
        protocol: Protocol::S3,
        ssh: None,
        sftp: tokio::sync::OnceCell::new(),
        ftp: None,
        s3: None,
        tar_available: tokio::sync::OnceCell::new(),
        lifecycle: Arc::new(SessionLifecycle::default()),
        watchdog: std::sync::Mutex::new(None),
    });
    manager
        .sessions
        .lock()
        .unwrap()
        .insert(entry.id.clone(), entry);
    let operation = manager.get("session").unwrap();

    let closing = manager.start_disconnect("session").unwrap();
    operation.operation().cancelled().await;

    let registered = AtomicBool::new(false);
    assert!(matches!(
        operation
            .operation()
            .register(|| registered.store(true, Ordering::SeqCst)),
        Err(AppError::SessionNotFound)
    ));
    assert!(!registered.load(Ordering::SeqCst));
    assert!(
        manager.get("session").is_err(),
        "closing session remained available for new operations"
    );
    drop(operation);
    tokio::time::timeout(Duration::from_secs(1), manager.finish_disconnect(closing))
        .await
        .expect("session close did not wait for and drain the operation");
    assert!(matches!(
        manager.get("session"),
        Err(AppError::SessionNotFound)
    ));
}

#[tokio::test]
async fn session_gate_rejects_registration_during_close() {
    assert_late_registration_is_rejected().await;
}

#[tokio::test]
async fn late_terminal_reader_is_rejected_and_joined() {
    let manager = Arc::new(SessionManager::default());
    let lifecycle = Arc::new(SessionLifecycle::default());
    let operation = lifecycle.try_begin_operation().unwrap();
    let close = tokio::spawn({
        let lifecycle = lifecycle.clone();
        async move { lifecycle.begin_close().await }
    });
    operation.cancelled().await;

    let (release, release_rx) = tokio::sync::oneshot::channel();
    let (stopped, stopped_rx) = tokio::sync::oneshot::channel();
    let reader_task = tokio::spawn(async move {
        let _ = release_rx.await;
        let _ = stopped.send(());
    });
    let (registered, registered_rx) = tokio::sync::oneshot::channel();
    let caller = tokio::spawn({
        let manager = manager.clone();
        async move {
            let result = manager
                .register_terminal(
                    &operation,
                    "terminal".into(),
                    TerminalEntry {
                        session_id: "session".into(),
                        write: None,
                        reader_task: Some(reader_task),
                        cleanup: operation.cleanup(),
                    },
                )
                .await;
            let _ = registered.send(matches!(result, Err(AppError::SessionNotFound)));
            std::future::pending::<()>().await;
        }
    });
    assert!(registered_rx.await.unwrap());
    caller.abort();
    let _ = caller.await;
    assert!(!close.is_finished());

    release.send(()).unwrap();
    stopped_rx
        .await
        .expect("rejected terminal reader was joined before returning");
    assert!(manager.terminals.lock().await.is_empty());
    close.await.unwrap().finish().await;
}

#[tokio::test]
async fn session_close_waits_for_finalizers_spawned_during_cancellation() {
    let lifecycle = Arc::new(SessionLifecycle::default());
    let operation = lifecycle.try_begin_operation().unwrap();
    let cleanup = operation.cleanup();
    let close = tokio::spawn({
        let lifecycle = lifecycle.clone();
        async move { lifecycle.begin_close().await }
    });
    operation.cancelled().await;

    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    assert!(cleanup
        .try_spawn(async move {
            let _ = started_tx.send(());
            let _ = release_rx.await;
        })
        .is_ok());
    drop(operation);
    started_rx.await.unwrap();
    assert!(!close.is_finished());

    release_tx.send(()).unwrap();
    let close_guard = tokio::time::timeout(Duration::from_secs(1), close)
        .await
        .expect("session close did not wait for its finalizer")
        .unwrap();
    close_guard.finish().await;
}

#[tokio::test]
async fn stalled_s3_operation_is_cancelled_by_session_close() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (started, started_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = socket.read(&mut buffer).await.unwrap();
            assert_ne!(read, 0);
            request.extend_from_slice(&buffer[..read]);
        }
        let _ = started.send(());
        std::future::pending::<()>().await;
    });

    let manager = Arc::new(SessionManager::default());
    let lifecycle = Arc::new(SessionLifecycle::default());
    let s3 = S3Fs::new_in_lifecycle(
        S3Config {
            endpoint: format!("http://{address}"),
            region: "us-east-1".into(),
            access_key: "access".into(),
            secret_key: zeroize::Zeroizing::new("secret".into()),
            bucket: Some("bucket".into()),
            path_style: true,
            upload_acl: S3UploadAcl::Private,
        },
        lifecycle.cleanup(),
    );
    manager.sessions.lock().unwrap().insert(
        "session".into(),
        Arc::new(SessionEntry {
            id: "session".into(),
            connection_id: "connection".into(),
            protocol: Protocol::S3,
            ssh: None,
            sftp: tokio::sync::OnceCell::new(),
            ftp: None,
            s3: Some(s3),
            tar_available: tokio::sync::OnceCell::new(),
            lifecycle: lifecycle.clone(),
            watchdog: std::sync::Mutex::new(None),
        }),
    );
    let operation = tokio::spawn({
        let manager = manager.clone();
        async move {
            let session = manager.get("session").unwrap();
            crate::commands::run_session_operation(&session, async {
                session.s3.as_ref().unwrap().probe().await
            })
            .await
        }
    });
    started_rx.await.unwrap();

    let closing = manager.start_disconnect("session").unwrap();
    let close = tokio::spawn({
        let manager = manager.clone();
        async move { manager.finish_disconnect(closing).await }
    });
    assert!(matches!(
        operation.await.unwrap(),
        Err(AppError::SessionNotFound)
    ));
    tokio::time::timeout(Duration::from_secs(1), close)
        .await
        .expect("session close did not cancel the stalled S3 operation")
        .unwrap();
    lifecycle.wait_closed().await;
    server.abort();
    let _ = server.await;
}
