//! Uploads must declare a `Content-Type` derived from the object name.
//!
//! Without it S3 stores `application/octet-stream` and browsers refuse the
//! stylesheets and modules of any site published through the panel.

use std::sync::Arc;

use serverus_lib::session::remote_fs::RemoteFs;
use serverus_lib::session::s3::S3Fs;
use tokio::io::AsyncWriteExt;

use super::common::{fs_for, head_content_type, MULTIPART_PART_SIZE};
use super::server::spawn_s3;

const BUCKET: &str = "uploads";

async fn upload(fs: &Arc<S3Fs>, path: &str, bytes: &[u8]) {
    let mut writer = fs.open_write(path, 0).await.unwrap();
    writer.write_all(bytes).await.unwrap();
    writer.shutdown().await.unwrap();
    drop(writer);
}

async fn fixture() -> (tempfile::TempDir, u16, Arc<S3Fs>) {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join(BUCKET)).unwrap();
    let port = spawn_s3(root.path()).await;
    let fs = fs_for(port, Some(BUCKET));
    (root, port, fs)
}

pub(crate) async fn put_object_uploads_declare_the_type() {
    let (_root, port, fs) = fixture().await;

    upload(&fs, "/app.css", b"body{color:red}").await;
    upload(&fs, "/app.js", b"export const a = 1;").await;
    upload(&fs, "/logo.svg", b"<svg/>").await;

    assert_eq!(
        head_content_type(port, BUCKET, "app.css").await.as_deref(),
        Some("text/css; charset=utf-8")
    );
    assert_eq!(
        head_content_type(port, BUCKET, "app.js").await.as_deref(),
        Some("text/javascript; charset=utf-8")
    );
    assert_eq!(
        head_content_type(port, BUCKET, "logo.svg").await.as_deref(),
        Some("image/svg+xml")
    );
}

pub(crate) async fn unknown_extensions_leave_the_sdk_default() {
    let (_root, port, fs) = fixture().await;

    upload(&fs, "/blob.unknownext", b"raw bytes").await;
    upload(&fs, "/Makefile", b"all:\n").await;

    // Neither the name nor the bytes identify these, so the SDK's own payload
    // default stands — the right answer for an unrecognized binary.
    assert_eq!(
        head_content_type(port, BUCKET, "blob.unknownext")
            .await
            .as_deref(),
        Some("application/octet-stream")
    );
    assert_eq!(
        head_content_type(port, BUCKET, "Makefile").await.as_deref(),
        Some("application/octet-stream")
    );
}

pub(crate) async fn nameless_files_are_identified_by_their_bytes() {
    let (_root, port, fs) = fixture().await;

    // No extension to go on, so the magic bytes answer instead.
    upload(&fs, "/photo", b"\xff\xd8\xff\xe0\x00\x10JFIF\0").await;
    upload(&fs, "/dump", b"SQLite format 3\0rest of the header").await;

    assert_eq!(
        head_content_type(port, BUCKET, "photo").await.as_deref(),
        Some("image/jpeg")
    );
    assert_eq!(
        head_content_type(port, BUCKET, "dump").await.as_deref(),
        Some("application/vnd.sqlite3")
    );
}

pub(crate) async fn the_name_outranks_the_bytes() {
    let (_root, port, fs) = fixture().await;

    // A stylesheet is plain text with no signature to find, so sniffing could
    // only ever downgrade it. The name has to win, or the original bug is
    // back for every file whose content is not self-describing.
    upload(&fs, "/app.css", b"body{color:red}").await;
    // And a name that resolves is never second-guessed, even when the bytes
    // clearly say something else.
    upload(&fs, "/mislabeled.css", b"\x89PNG\r\n\x1a\n").await;

    assert_eq!(
        head_content_type(port, BUCKET, "app.css").await.as_deref(),
        Some("text/css; charset=utf-8")
    );
    assert_eq!(
        head_content_type(port, BUCKET, "mislabeled.css")
            .await
            .as_deref(),
        Some("text/css; charset=utf-8")
    );
}

pub(crate) async fn multipart_uploads_sniff_their_head() {
    let (_root, port, fs) = fixture().await;

    // Past the part threshold the type is declared by CreateMultipartUpload,
    // which is built while the first part is still buffered.
    let mut bytes = b"\x1f\x8b\x08\x00".to_vec();
    bytes.resize(MULTIPART_PART_SIZE + 1, 0);
    upload(&fs, "/archive", &bytes).await;

    assert_eq!(
        head_content_type(port, BUCKET, "archive").await.as_deref(),
        Some("application/gzip")
    );
}

pub(crate) async fn multipart_uploads_declare_the_type() {
    let (_root, port, fs) = fixture().await;

    // Past the part threshold the object is created by CreateMultipartUpload,
    // a different request from the small-file PutObject path.
    upload(&fs, "/bundle.js", &vec![b'x'; MULTIPART_PART_SIZE + 1]).await;

    assert_eq!(
        head_content_type(port, BUCKET, "bundle.js")
            .await
            .as_deref(),
        Some("text/javascript; charset=utf-8")
    );
}

pub(crate) async fn created_empty_files_declare_the_type() {
    let (_root, port, fs) = fixture().await;

    fs.create_file("/new.css").await.unwrap();

    assert_eq!(
        head_content_type(port, BUCKET, "new.css").await.as_deref(),
        Some("text/css; charset=utf-8")
    );
}

pub(crate) async fn remote_edit_publishes_the_targets_type_not_the_staging_name() {
    let (_root, port, fs) = fixture().await;
    upload(&fs, "/app.css", b"body{color:red}").await;

    // The remote-edit watcher stages under a `.tmp` sibling and promotes it;
    // neither step may leave the published object typed after `.tmp`.
    let staged = "/.serverus-edit-00000000-0000-4000-8000-000000000000.tmp";
    let mut writer = fs.open_write_replacement(staged, "/app.css").await.unwrap();
    writer.write_all(b"body{color:blue}").await.unwrap();
    writer.shutdown().await.unwrap();
    drop(writer);

    assert_eq!(
        head_content_type(port, BUCKET, staged.trim_start_matches('/'))
            .await
            .as_deref(),
        Some("text/css; charset=utf-8"),
        "the staged object is typed after the target, not after `.tmp`"
    );

    fs.replace_file(staged, "/app.css").await.unwrap();

    assert_eq!(
        head_content_type(port, BUCKET, "app.css").await.as_deref(),
        Some("text/css; charset=utf-8")
    );
}
