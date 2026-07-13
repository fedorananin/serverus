//! THE founding test of the project (SPEC §4.3): recursive FTP directory
//! operations must always work. Runs against a real in-process FTP server
//! (libunftp with a filesystem backend).

use std::fs;
use std::net::TcpListener;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serverus_lib::session::ftp::{FtpConfig, FtpPool};
use serverus_lib::session::remote_fs::{delete_recursive, RemoteFs};
use serverus_lib::transfer::{ProgressSink, TransferManager, TransferState};
use serverus_lib::vault::model::{ConflictPolicy, FtpTlsMode, TransferSettings};
use unftp_sbe_fs::Filesystem;
use zeroize::Zeroizing;

struct NullSink;
impl ProgressSink for NullSink {
    fn emit(&self, _event: serverus_lib::events::TransferProgressEvent) {}
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Spawn an in-process FTP server rooted at `root`; returns its port.
async fn spawn_ftp(root: &Path) -> u16 {
    let port = free_port();
    let root = root.to_path_buf();
    let server =
        libunftp::ServerBuilder::new(Box::new(move || Filesystem::new(root.clone()).unwrap()))
            .passive_ports(40000..=49999)
            .build()
            .unwrap();
    let addr = format!("127.0.0.1:{port}");
    tokio::spawn(async move {
        let _ = server.listen(addr).await;
    });
    for _ in 0..100 {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return port;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("ftp server did not start");
}

fn pool_for(port: u16) -> Arc<FtpPool> {
    FtpPool::new(
        FtpConfig {
            host: "127.0.0.1".into(),
            port,
            username: "anonymous".into(),
            password: Zeroizing::new(String::new()),
            tls: FtpTlsMode::None,
            passive: true,
        },
        4,
    )
}

fn settings() -> TransferSettings {
    TransferSettings {
        max_parallel_per_server: 4,
        conflict_policy: ConflictPolicy::Overwrite,
        preserve_mtime: false, // MFMT not supported by libunftp — best effort
        tar_acceleration: false,
    }
}

async fn wait_for_drain(manager: &Arc<TransferManager>) {
    for _ in 0..600 {
        let (_, summary) = manager.snapshot();
        if summary.queued == 0 && summary.running == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (items, summary) = manager.snapshot();
    panic!("queue did not drain: {summary:?}\n{items:#?}");
}

fn assert_all_done(manager: &Arc<TransferManager>) {
    let (items, _) = manager.snapshot();
    for item in &items {
        assert!(
            matches!(item.state, TransferState::Done),
            "item not done: {item:#?}"
        );
    }
}

#[tokio::test]
async fn ftp_basic_operations() {
    let server_root = tempfile::tempdir().unwrap();
    let port = spawn_ftp(server_root.path()).await;
    let pool = pool_for(port);

    pool.probe().await.unwrap();
    let home = pool.home_dir().await.unwrap();
    assert_eq!(home, "/");

    pool.mkdir("/dir").await.unwrap();
    pool.create_file("/dir/a.txt").await.unwrap();
    pool.rename("/dir/a.txt", "/dir/b.txt").await.unwrap();

    let listing = pool.list("/dir").await.unwrap();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].name, "b.txt");
    assert!(!listing[0].is_dir);

    assert!(pool.exists("/dir/b.txt").await.unwrap());
    assert!(!pool.exists("/dir/ghost.txt").await.unwrap());

    pool.delete_file("/dir/b.txt").await.unwrap();
    pool.delete_dir("/dir").await.unwrap();
    assert!(!pool.exists("/dir").await.unwrap());
}

/// The electerm bug that started this project: uploading, downloading and
/// deleting a nested directory tree over FTP — any depth — must just work.
#[tokio::test]
async fn ftp_recursive_directory_roundtrip() {
    let server_root = tempfile::tempdir().unwrap();
    let port = spawn_ftp(server_root.path()).await;
    let pool: Arc<dyn RemoteFs> = pool_for(port);

    // Nested local tree with an empty dir, empty file and binary data.
    let src_root = tempfile::tempdir().unwrap();
    let tree = src_root.path().join("site");
    fs::create_dir_all(tree.join("assets/img/icons")).unwrap();
    fs::create_dir_all(tree.join("empty")).unwrap();
    fs::write(tree.join("index.html"), b"<html>hi</html>").unwrap();
    fs::write(tree.join("assets/app.css"), vec![b'x'; 50_000]).unwrap();
    fs::write(
        tree.join("assets/img/icons/logo.png"),
        (0..=255u8).cycle().take(200_000).collect::<Vec<_>>(),
    )
    .unwrap();
    fs::write(tree.join("assets/img/icons/empty.txt"), b"").unwrap();

    let manager = Arc::new(TransferManager::default());
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);

    // Recursive upload.
    manager
        .enqueue_upload(
            &sink,
            pool.clone(),
            "ftp-1",
            tree.to_str().unwrap(),
            "/",
            settings(),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);
    manager.clear_finished();

    // Verify structure server-side (through the backend's own listing).
    let icons = pool.list("/site/assets/img/icons").await.unwrap();
    let names: Vec<_> = icons.iter().map(|e| e.name.clone()).collect();
    assert!(names.contains(&"logo.png".to_string()), "{names:?}");
    assert!(names.contains(&"empty.txt".to_string()), "{names:?}");
    assert!(pool.exists("/site/empty").await.unwrap());

    // Recursive download into a fresh dir and byte-compare.
    let dst_root = tempfile::tempdir().unwrap();
    manager
        .enqueue_download(
            &sink,
            pool.clone(),
            "ftp-1",
            "/site",
            dst_root.path().to_str().unwrap(),
            settings(),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);

    for rel in [
        "index.html",
        "assets/app.css",
        "assets/img/icons/logo.png",
        "assets/img/icons/empty.txt",
    ] {
        let original = fs::read(tree.join(rel)).unwrap();
        let copied = fs::read(dst_root.path().join("site").join(rel)).unwrap();
        assert_eq!(original, copied, "content mismatch for {rel}");
    }
    assert!(dst_root.path().join("site/empty").is_dir());

    // Recursive delete of the whole tree on the server.
    delete_recursive(pool.as_ref(), "/site", true)
        .await
        .unwrap();
    assert!(!pool.exists("/site").await.unwrap());
    // The server root must actually be empty on disk.
    assert_eq!(fs::read_dir(server_root.path()).unwrap().count(), 0);
}

/// Resume (REST): reading from an offset yields the tail of the file.
#[tokio::test]
async fn ftp_read_with_offset() {
    let server_root = tempfile::tempdir().unwrap();
    fs::write(server_root.path().join("data.bin"), b"0123456789").unwrap();
    let port = spawn_ftp(server_root.path()).await;
    let pool = pool_for(port);

    let mut reader = pool.open_read("/data.bin", 4).await.unwrap();
    let mut buf = Vec::new();
    use tokio::io::AsyncReadExt;
    reader.read_to_end(&mut buf).await.unwrap();
    assert_eq!(buf, b"456789");
}
