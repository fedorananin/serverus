use std::sync::Arc;

use serverus_lib::session::remote_fs::{delete_recursive, RemoteFs};
use serverus_lib::session::s3::{S3AclStatus, S3AclTarget};
use serverus_lib::transfer::{ProgressSink, TransferManager, UploadRequest};
use serverus_lib::vault::model::S3UploadAcl;

use super::common::{assert_all_done, fs_for, settings, statuses, wait_for_drain, NullSink};
use super::server::spawn_s3;

pub(crate) async fn bucket_level_operations() {
    let root = tempfile::tempdir().unwrap();
    let port = spawn_s3(root.path()).await;
    let fs = fs_for(port, None);

    fs.probe().await.unwrap();
    assert_eq!(fs.home_dir().await.unwrap(), "/");

    fs.mkdir("/first-bucket").await.unwrap();
    fs.mkdir("/second-bucket").await.unwrap();
    let roots = fs.list("/").await.unwrap();
    let names: Vec<&str> = roots.iter().map(|entry| entry.name.as_str()).collect();
    assert!(names.contains(&"first-bucket"), "{names:?}");
    assert!(names.contains(&"second-bucket"), "{names:?}");
    assert!(roots.iter().all(|entry| entry.is_dir));

    fs.mkdir("/first-bucket/docs").await.unwrap();
    fs.create_file("/first-bucket/docs/a.txt").await.unwrap();
    assert!(fs.exists("/first-bucket/docs/a.txt").await.unwrap());
    assert!(fs.exists("/first-bucket/docs").await.unwrap());
    assert!(!fs.exists("/first-bucket/missing").await.unwrap());

    let listing = fs.list("/first-bucket/docs").await.unwrap();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].name, "a.txt");
    assert!(!listing[0].is_dir);
    assert!(fs.create_file("/first-bucket/docs/a.txt").await.is_err());

    fs.rename("/first-bucket/docs/a.txt", "/first-bucket/docs/b.txt")
        .await
        .unwrap();
    assert!(!fs.exists("/first-bucket/docs/a.txt").await.unwrap());
    assert!(fs.exists("/first-bucket/docs/b.txt").await.unwrap());

    fs.set_acl(
        vec![S3AclTarget {
            path: "/first-bucket/docs/b.txt".into(),
            is_dir: false,
        }],
        true,
    )
    .await
    .unwrap();
    let mut staged = fs
        .open_write_replacement("/first-bucket/docs/edit-staged", "/first-bucket/docs/b.txt")
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut staged, b"new contents")
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::shutdown(&mut staged)
        .await
        .unwrap();
    drop(staged);
    fs.replace_file("/first-bucket/docs/edit-staged", "/first-bucket/docs/b.txt")
        .await
        .unwrap();
    assert!(!fs.exists("/first-bucket/docs/edit-staged").await.unwrap());
    let mut reader = fs.open_read("/first-bucket/docs/b.txt", 0).await.unwrap();
    let mut replaced = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut reader, &mut replaced)
        .await
        .unwrap();
    assert_eq!(replaced, b"new contents");
    let status = statuses(
        &fs.acl_status_batch(vec!["/first-bucket/docs/b.txt".into()])
            .await,
    );
    assert_eq!(status["/first-bucket/docs/b.txt"], S3AclStatus::Public);

    assert!(fs.chmod("/first-bucket/docs/b.txt", 0o644).await.is_err());
    fs.set_mtime("/first-bucket/docs/b.txt", 0).await.unwrap();

    delete_recursive(fs.as_ref(), "/first-bucket", true)
        .await
        .unwrap();
    let names: Vec<String> = fs
        .list("/")
        .await
        .unwrap()
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert!(!names.contains(&"first-bucket".to_string()), "{names:?}");
}

pub(crate) async fn acl_public_private_flow() {
    let root = tempfile::tempdir().unwrap();
    let port = spawn_s3(root.path()).await;
    std::fs::create_dir(root.path().join("acl")).unwrap();
    let fs = fs_for(port, Some("acl"));

    fs.set_upload_acl(S3UploadAcl::PublicRead);
    fs.create_file("/public.txt").await.unwrap();
    fs.set_upload_acl(S3UploadAcl::Private);
    fs.create_file("/private.txt").await.unwrap();

    let status = statuses(
        &fs.acl_status_batch(vec!["/public.txt".into(), "/private.txt".into()])
            .await,
    );
    assert_eq!(status["/public.txt"], S3AclStatus::Public);
    assert_eq!(status["/private.txt"], S3AclStatus::Private);

    fs.set_upload_acl(S3UploadAcl::PublicRead);
    let payload = vec![3_u8; 9 * 1024 * 1024];
    let local = tempfile::tempdir().unwrap();
    std::fs::write(local.path().join("big-public.bin"), &payload).unwrap();
    let manager = Arc::new(TransferManager::default());
    let context_id = crate::transfer_context::activate(&manager);
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    manager
        .enqueue_upload(
            context_id,
            &sink,
            UploadRequest::new(
                fs.clone(),
                "session",
                local.path().join("big-public.bin").to_str().unwrap(),
                "/",
                settings(),
            ),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);
    let status = statuses(&fs.acl_status_batch(vec!["/big-public.bin".into()]).await);
    assert_eq!(status["/big-public.bin"], S3AclStatus::Public);

    fs.mkdir("/docs").await.unwrap();
    fs.set_upload_acl(S3UploadAcl::Private);
    fs.create_file("/docs/a.txt").await.unwrap();
    fs.create_file("/docs/b.txt").await.unwrap();
    let changed = fs
        .set_acl(
            vec![S3AclTarget {
                path: "/docs".into(),
                is_dir: true,
            }],
            true,
        )
        .await
        .unwrap();
    assert_eq!(changed, 2);
    let status = statuses(
        &fs.acl_status_batch(vec!["/docs/a.txt".into(), "/docs/b.txt".into()])
            .await,
    );
    assert_eq!(status["/docs/a.txt"], S3AclStatus::Public);
    assert_eq!(status["/docs/b.txt"], S3AclStatus::Public);

    let changed = fs
        .set_acl(
            vec![S3AclTarget {
                path: "/docs/a.txt".into(),
                is_dir: false,
            }],
            false,
        )
        .await
        .unwrap();
    assert_eq!(changed, 1);
    let status = statuses(&fs.acl_status_batch(vec!["/docs/a.txt".into()]).await);
    assert_eq!(status["/docs/a.txt"], S3AclStatus::Private);

    let changed = fs
        .set_acl(
            vec![S3AclTarget {
                path: "/public.txt".into(),
                is_dir: false,
            }],
            true,
        )
        .await
        .unwrap();
    assert_eq!(changed, 1);
    fs.rename("/public.txt", "/renamed.txt").await.unwrap();
    let status = statuses(&fs.acl_status_batch(vec!["/renamed.txt".into()]).await);
    assert_eq!(status["/renamed.txt"], S3AclStatus::Public);
}

pub(crate) async fn replacement_staging_is_private_under_public_upload_mode() {
    let root = tempfile::tempdir().unwrap();
    let port = spawn_s3(root.path()).await;
    std::fs::create_dir(root.path().join("replacement-acl")).unwrap();
    let fs = fs_for(port, Some("replacement-acl"));

    fs.set_upload_acl(S3UploadAcl::PublicRead);
    fs.create_file("/target.txt").await.unwrap();

    let mut staged = fs
        .open_write_replacement("/.serverus-edit-staged", "/target.txt")
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::write_all(&mut staged, b"private staging bytes")
        .await
        .unwrap();
    tokio::io::AsyncWriteExt::shutdown(&mut staged)
        .await
        .unwrap();
    drop(staged);

    let status = statuses(
        &fs.acl_status_batch(vec!["/target.txt".into(), "/.serverus-edit-staged".into()])
            .await,
    );
    assert_eq!(status["/target.txt"], S3AclStatus::Public);
    assert_eq!(status["/.serverus-edit-staged"], S3AclStatus::Private);
}
