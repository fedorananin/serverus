use std::fs;

use serverus_lib::session::remote_fs::{delete_recursive, join_remote, RemoteFs};
use serverus_lib::session::sftp::SftpFs;

use super::common::connect;
use crate::support::TestSshd;

pub(crate) async fn basic_operations() {
    let sshd = TestSshd::spawn();
    let session = connect(&sshd).await;
    let fs_remote = SftpFs::open(&session).await.unwrap();

    let home = fs_remote.home_dir().await.unwrap();
    assert!(home.starts_with('/'));

    let scratch = sshd.dir.path().to_string_lossy().into_owned();
    let base = join_remote(&scratch, "sftp-ops");
    fs_remote.mkdir(&base).await.unwrap();
    assert!(fs_remote.exists(&base).await.unwrap());

    let file_a = join_remote(&base, "a.txt");
    let file_b = join_remote(&base, "b.txt");
    fs_remote.create_file(&file_a).await.unwrap();
    fs_remote.rename(&file_a, &file_b).await.unwrap();
    let entry = fs_remote.stat(&file_b).await.unwrap();
    assert!(!entry.is_dir);

    fs_remote.chmod(&file_b, 0o640).await.unwrap();
    let entry = fs_remote.stat(&file_b).await.unwrap();
    assert_eq!(entry.permissions.unwrap() & 0o777, 0o640);

    let staged = join_remote(&base, ".edit-staged");
    let mut writer = fs_remote
        .open_write_replacement(&staged, &file_b)
        .await
        .unwrap();
    let staged_entry = fs_remote.stat(&staged).await.unwrap();
    assert_eq!(staged_entry.permissions.unwrap() & 0o777, 0o640);
    tokio::io::AsyncWriteExt::write_all(&mut writer, b"new contents")
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::shutdown(&mut writer)
        .await
        .unwrap();
    drop(writer);
    fs_remote.replace_file(&staged, &file_b).await.unwrap();
    assert_eq!(fs::read(&file_b).unwrap(), b"new contents");
    let entry = fs_remote.stat(&file_b).await.unwrap();
    assert_eq!(entry.permissions.unwrap() & 0o777, 0o640);
    assert!(!fs_remote.exists(&staged).await.unwrap());

    let listing = fs_remote.list(&base).await.unwrap();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].name, "b.txt");

    delete_recursive(&fs_remote, &base, true).await.unwrap();
    assert!(!fs_remote.exists(&base).await.unwrap());
}
