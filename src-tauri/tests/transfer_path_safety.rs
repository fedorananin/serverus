//! Recursive downloads must treat every remote name as an untrusted path
//! component and keep every local write beneath the selected destination.

use std::collections::HashMap;
use std::sync::Arc;

use serverus_lib::error::{AppError, AppResult};
use serverus_lib::events::TransferProgressEvent;
use serverus_lib::session::remote_fs::{BoxRead, BoxWrite, RemoteEntry, RemoteFs};
use serverus_lib::transfer::{ProgressSink, TransferManager, TransferState};
use serverus_lib::vault::model::{ConflictPolicy, TransferSettings};

struct NullSink;

impl ProgressSink for NullSink {
    fn emit(&self, _event: TransferProgressEvent) {}
}

struct ListingFs {
    root: RemoteEntry,
    listings: HashMap<String, Vec<RemoteEntry>>,
}

impl ListingFs {
    fn file(name: &str) -> Arc<Self> {
        Arc::new(Self {
            root: entry(name, "/tree", false, false),
            listings: HashMap::new(),
        })
    }

    fn directory_with_children(names: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            root: entry("tree", "/tree", true, false),
            listings: HashMap::from([(
                "/tree".into(),
                names
                    .iter()
                    .enumerate()
                    // Unique remote paths even for duplicate names, so the
                    // mock's stat() lookup stays unambiguous.
                    .map(|(i, name)| entry(name, &format!("/tree/{i}-{name}"), false, false))
                    .collect(),
            )]),
        })
    }

    fn directory_with_nested_file(directory: &str, file: &str) -> Arc<Self> {
        Arc::new(Self {
            root: entry("tree", "/tree", true, false),
            listings: HashMap::from([
                (
                    "/tree".into(),
                    vec![entry(directory, &format!("/tree/{directory}"), true, false)],
                ),
                (
                    format!("/tree/{directory}"),
                    vec![entry(
                        file,
                        &format!("/tree/{directory}/{file}"),
                        false,
                        false,
                    )],
                ),
            ]),
        })
    }
}

fn entry(name: &str, path: &str, is_dir: bool, is_symlink: bool) -> RemoteEntry {
    RemoteEntry {
        name: name.into(),
        path: path.into(),
        is_dir,
        is_symlink,
        size: 6,
        mtime: None,
        permissions: None,
    }
}

#[async_trait::async_trait]
impl RemoteFs for ListingFs {
    async fn list(&self, path: &str) -> AppResult<Vec<RemoteEntry>> {
        Ok(self.listings.get(path).cloned().unwrap_or_default())
    }

    async fn stat(&self, path: &str) -> AppResult<RemoteEntry> {
        if path == "/tree" {
            Ok(self.root.clone())
        } else if let Some(entry) = self
            .listings
            .values()
            .flatten()
            .find(|entry| entry.path == path)
        {
            Ok(entry.clone())
        } else {
            Err(AppError::RemoteFs(format!("{path}: not found")))
        }
    }

    async fn home_dir(&self) -> AppResult<String> {
        Ok("/".into())
    }

    async fn mkdir(&self, _path: &str) -> AppResult<()> {
        Ok(())
    }

    async fn create_file(&self, _path: &str) -> AppResult<()> {
        Ok(())
    }

    async fn rename(&self, _from: &str, _to: &str) -> AppResult<()> {
        Ok(())
    }

    async fn delete_file(&self, _path: &str) -> AppResult<()> {
        Ok(())
    }

    async fn delete_dir(&self, _path: &str) -> AppResult<()> {
        Ok(())
    }

    async fn chmod(&self, _path: &str, _mode: u32) -> AppResult<()> {
        Ok(())
    }

    async fn set_mtime(&self, _path: &str, _mtime_unix: i64) -> AppResult<()> {
        Ok(())
    }

    async fn open_read(&self, _path: &str, _offset: u64) -> AppResult<BoxRead> {
        Ok(Box::new(std::io::Cursor::new(b"attack".to_vec())))
    }

    async fn open_write(&self, path: &str, _offset: u64) -> AppResult<BoxWrite> {
        Err(AppError::RemoteFs(format!("{path}: not supported")))
    }

    async fn exists(&self, _path: &str) -> AppResult<bool> {
        Ok(false)
    }
}

fn settings() -> TransferSettings {
    TransferSettings {
        max_parallel_per_server: 1,
        conflict_policy: ConflictPolicy::Overwrite,
        preserve_mtime: false,
        tar_acceleration: false,
    }
}

async fn enqueue(
    fs: Arc<ListingFs>,
    destination: &std::path::Path,
) -> AppResult<Arc<TransferManager>> {
    let manager = Arc::new(TransferManager::default());
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    manager
        .enqueue_download(
            &sink,
            fs,
            "session",
            "/tree",
            destination.to_str().unwrap(),
            settings(),
        )
        .await?;
    Ok(manager)
}

async fn wait_for_transfer(manager: &TransferManager) {
    for _ in 0..100 {
        let (items, _) = manager.snapshot();
        if !items.is_empty()
            && items.iter().all(|item| {
                matches!(
                    item.state,
                    TransferState::Done | TransferState::Error | TransferState::Cancelled
                )
            })
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("transfer did not finish");
}

#[tokio::test]
async fn one_attack_shaped_child_name_fails_alone_not_the_tree() {
    for name in [
        ".",
        "..",
        "../../escape.txt",
        "/absolute.txt",
        "nested/file.txt",
    ] {
        let destination = tempfile::tempdir().unwrap();
        let manager = enqueue(
            ListingFs::directory_with_children(&[name, "good.txt"]),
            destination.path(),
        )
        .await
        .unwrap_or_else(|e| panic!("{name:?} aborted the whole tree: {e}"));
        wait_for_transfer(&manager).await;

        let (items, _) = manager.snapshot();
        assert_eq!(items.len(), 2, "{name:?}");
        let failed = items
            .iter()
            .find(|item| item.state == TransferState::Error)
            .unwrap_or_else(|| panic!("no failed item for {name:?}"));
        assert!(failed.error.as_deref().unwrap().contains("unsafe"));
        assert!(items.iter().any(|item| item.state == TransferState::Done));
        // The good sibling downloaded; nothing escaped or used the bad name.
        assert_eq!(
            std::fs::read(destination.path().join("tree/good.txt")).unwrap(),
            b"attack"
        );
        assert_eq!(
            std::fs::read_dir(destination.path().join("tree"))
                .unwrap()
                .count(),
            1,
            "{name:?} left an artifact"
        );
    }
}

#[tokio::test]
async fn duplicate_local_names_fail_only_the_duplicate() {
    let destination = tempfile::tempdir().unwrap();
    let manager = enqueue(
        ListingFs::directory_with_children(&["dup.txt", "dup.txt"]),
        destination.path(),
    )
    .await
    .unwrap();
    wait_for_transfer(&manager).await;

    let (items, _) = manager.snapshot();
    assert_eq!(items.len(), 2);
    assert!(items.iter().any(|item| item.state == TransferState::Done));
    let failed = items
        .iter()
        .find(|item| item.state == TransferState::Error)
        .expect("the colliding entry should fail");
    assert!(failed
        .error
        .as_deref()
        .unwrap()
        .contains("already maps to this local name"));
    assert_eq!(
        std::fs::read(destination.path().join("tree/dup.txt")).unwrap(),
        b"attack"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn unix_legal_names_download_verbatim() {
    // Every one of these is a legal file name on mac/linux; refusing them
    // (or renaming them) broke the founding recursive-download use case.
    let names = [
        "2024-01-01T12:00:00.log",
        "file:stream",
        "CON",
        "aux.txt",
        "trailing.",
        "trailing ",
        "wild*card",
        r"nested\file.txt",
        "C:drive-relative.txt",
    ];
    let destination = tempfile::tempdir().unwrap();
    let manager = enqueue(
        ListingFs::directory_with_children(&names),
        destination.path(),
    )
    .await
    .unwrap();
    wait_for_transfer(&manager).await;

    let (items, _) = manager.snapshot();
    assert!(
        items.iter().all(|item| item.state == TransferState::Done),
        "some legal name failed: {items:?}"
    );
    for name in names {
        assert_eq!(
            std::fs::read(destination.path().join("tree").join(name)).unwrap(),
            b"attack",
            "{name:?} was renamed or skipped"
        );
    }
}

#[tokio::test]
async fn download_rejects_unsafe_top_level_names() {
    let destination = tempfile::tempdir().unwrap();

    for name in ["..", ".", "/absolute.txt", "nested/file.txt"] {
        let result = enqueue(ListingFs::file(name), destination.path()).await;
        assert!(
            result.is_err(),
            "unsafe top-level name was accepted: {name:?}"
        );
    }
}

#[tokio::test]
async fn valid_recursive_download_stays_under_the_destination() {
    let destination = tempfile::tempdir().unwrap();
    let manager = enqueue(
        ListingFs::directory_with_nested_file("nested", "safe.txt"),
        destination.path(),
    )
    .await
    .unwrap();
    wait_for_transfer(&manager).await;

    assert_eq!(
        std::fs::read(destination.path().join("tree/nested/safe.txt")).unwrap(),
        b"attack"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn download_does_not_follow_an_existing_file_symlink() {
    use std::os::unix::fs::symlink;

    let destination = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let outside_file = outside.path().join("victim.txt");
    std::fs::write(&outside_file, "keep me").unwrap();
    symlink(&outside_file, destination.path().join("victim.txt")).unwrap();

    let manager = enqueue(ListingFs::file("victim.txt"), destination.path())
        .await
        .unwrap();
    wait_for_transfer(&manager).await;

    assert_eq!(std::fs::read_to_string(outside_file).unwrap(), "keep me");
    assert_eq!(manager.snapshot().0[0].state, TransferState::Error);
}

#[cfg(unix)]
#[tokio::test]
async fn recursive_download_does_not_follow_a_directory_symlink() {
    use std::os::unix::fs::symlink;

    let destination = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let local_root = destination.path().join("tree");
    std::fs::create_dir(&local_root).unwrap();
    let outside_file = outside.path().join("victim.txt");
    std::fs::write(&outside_file, "keep me").unwrap();
    symlink(outside.path(), local_root.join("escape")).unwrap();

    let result = enqueue(
        ListingFs::directory_with_nested_file("escape", "victim.txt"),
        destination.path(),
    )
    .await;

    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(outside_file).unwrap(), "keep me");
}
