//! Recursive downloads must treat every remote name as an untrusted path
//! component and keep every local write beneath the selected destination.

#[path = "transfer_path_safety/fixtures.rs"]
mod fixtures;
#[path = "support/transfer_context.rs"]
mod transfer_context;

use fixtures::{enqueue, wait_for_transfer, ListingFs};
use serverus_lib::transfer::TransferState;

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
        .unwrap_or_else(|error| panic!("{name:?} aborted the whole tree: {error}"));
        wait_for_transfer(&manager).await;

        let (items, _) = manager.snapshot();
        assert_eq!(items.len(), 2, "{name:?}");
        let failed = items
            .iter()
            .find(|item| item.state == TransferState::Error)
            .unwrap_or_else(|| panic!("no failed item for {name:?}"));
        assert!(failed.error.as_deref().unwrap().contains("unsafe"));
        assert!(items.iter().any(|item| item.state == TransferState::Done));
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
