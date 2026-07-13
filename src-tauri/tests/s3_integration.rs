//! S3 integration tests (SPEC §4.4, §7.3): file operations, recursive
//! transfers and public/private ACLs against a real in-process S3 server
//! (`s3s` + filesystem backend). s3s-fs does not implement ACL operations,
//! so a thin wrapper keeps object ACLs in memory — enough to verify what
//! Serverus sends and reads.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use s3s::auth::SimpleAuth;
use s3s::dto;
use s3s::service::S3ServiceBuilder;
use s3s::{S3Request, S3Response, S3Result, S3};

use serverus_lib::session::remote_fs::{delete_recursive, RemoteFs};
use serverus_lib::session::s3::{S3AclStatus, S3AclTarget, S3Config, S3Fs};
use serverus_lib::transfer::{ProgressSink, TransferManager, TransferState};
use serverus_lib::vault::model::{ConflictPolicy, S3UploadAcl, TransferSettings};
use zeroize::Zeroizing;

const ACCESS_KEY: &str = "serverus-test-key";
const SECRET_KEY: &str = "serverus-test-secret";
const ALL_USERS: &str = "http://acs.amazonaws.com/groups/global/AllUsers";

struct NullSink;
impl ProgressSink for NullSink {
    fn emit(&self, _event: serverus_lib::events::TransferProgressEvent) {}
}

// ---------------------------------------------------------------------------
// s3s-fs wrapper adding in-memory per-object ACLs
// ---------------------------------------------------------------------------

struct AclFs {
    inner: s3s_fs::FileSystem,
    /// (bucket, key) pairs currently public-read.
    public: Mutex<HashSet<(String, String)>>,
    /// upload_id → (bucket, key, public) for multipart uploads with an ACL.
    pending: Mutex<HashMap<String, (String, String, bool)>>,
}

impl AclFs {
    fn new(inner: s3s_fs::FileSystem) -> AclFs {
        AclFs {
            inner,
            public: Mutex::new(HashSet::new()),
            pending: Mutex::new(HashMap::new()),
        }
    }

    fn set_public(&self, bucket: &str, key: &str, public: bool) {
        let mut set = self.public.lock().unwrap();
        if public {
            set.insert((bucket.to_string(), key.to_string()));
        } else {
            set.remove(&(bucket.to_string(), key.to_string()));
        }
    }

    fn is_public_acl(acl: Option<&dto::ObjectCannedACL>) -> bool {
        acl.map(|a| a.as_str() == dto::ObjectCannedACL::PUBLIC_READ)
            .unwrap_or(false)
    }
}

#[async_trait::async_trait]
impl S3 for AclFs {
    async fn list_buckets(
        &self,
        req: S3Request<dto::ListBucketsInput>,
    ) -> S3Result<S3Response<dto::ListBucketsOutput>> {
        self.inner.list_buckets(req).await
    }

    async fn create_bucket(
        &self,
        req: S3Request<dto::CreateBucketInput>,
    ) -> S3Result<S3Response<dto::CreateBucketOutput>> {
        self.inner.create_bucket(req).await
    }

    async fn delete_bucket(
        &self,
        req: S3Request<dto::DeleteBucketInput>,
    ) -> S3Result<S3Response<dto::DeleteBucketOutput>> {
        self.inner.delete_bucket(req).await
    }

    async fn head_bucket(
        &self,
        req: S3Request<dto::HeadBucketInput>,
    ) -> S3Result<S3Response<dto::HeadBucketOutput>> {
        self.inner.head_bucket(req).await
    }

    async fn list_objects_v2(
        &self,
        req: S3Request<dto::ListObjectsV2Input>,
    ) -> S3Result<S3Response<dto::ListObjectsV2Output>> {
        self.inner.list_objects_v2(req).await
    }

    async fn head_object(
        &self,
        req: S3Request<dto::HeadObjectInput>,
    ) -> S3Result<S3Response<dto::HeadObjectOutput>> {
        self.inner.head_object(req).await
    }

    async fn get_object(
        &self,
        req: S3Request<dto::GetObjectInput>,
    ) -> S3Result<S3Response<dto::GetObjectOutput>> {
        self.inner.get_object(req).await
    }

    async fn put_object(
        &self,
        req: S3Request<dto::PutObjectInput>,
    ) -> S3Result<S3Response<dto::PutObjectOutput>> {
        let bucket = req.input.bucket.clone();
        let key = req.input.key.clone();
        let public = Self::is_public_acl(req.input.acl.as_ref());
        let resp = self.inner.put_object(req).await?;
        self.set_public(&bucket, &key, public);
        Ok(resp)
    }

    async fn delete_object(
        &self,
        req: S3Request<dto::DeleteObjectInput>,
    ) -> S3Result<S3Response<dto::DeleteObjectOutput>> {
        let bucket = req.input.bucket.clone();
        let key = req.input.key.clone();
        let resp = self.inner.delete_object(req).await?;
        self.set_public(&bucket, &key, false);
        Ok(resp)
    }

    async fn copy_object(
        &self,
        req: S3Request<dto::CopyObjectInput>,
    ) -> S3Result<S3Response<dto::CopyObjectOutput>> {
        // Like real S3: the copy does NOT inherit the source ACL.
        let bucket = req.input.bucket.clone();
        let key = req.input.key.clone();
        let resp = self.inner.copy_object(req).await?;
        self.set_public(&bucket, &key, false);
        Ok(resp)
    }

    async fn create_multipart_upload(
        &self,
        req: S3Request<dto::CreateMultipartUploadInput>,
    ) -> S3Result<S3Response<dto::CreateMultipartUploadOutput>> {
        let bucket = req.input.bucket.clone();
        let key = req.input.key.clone();
        let public = Self::is_public_acl(req.input.acl.as_ref());
        let resp = self.inner.create_multipart_upload(req).await?;
        if let Some(upload_id) = &resp.output.upload_id {
            self.pending
                .lock()
                .unwrap()
                .insert(upload_id.clone(), (bucket, key, public));
        }
        Ok(resp)
    }

    async fn upload_part(
        &self,
        req: S3Request<dto::UploadPartInput>,
    ) -> S3Result<S3Response<dto::UploadPartOutput>> {
        self.inner.upload_part(req).await
    }

    async fn complete_multipart_upload(
        &self,
        req: S3Request<dto::CompleteMultipartUploadInput>,
    ) -> S3Result<S3Response<dto::CompleteMultipartUploadOutput>> {
        let upload_id = req.input.upload_id.clone();
        let resp = self.inner.complete_multipart_upload(req).await?;
        if let Some((bucket, key, public)) = self.pending.lock().unwrap().remove(&upload_id) {
            self.set_public(&bucket, &key, public);
        }
        Ok(resp)
    }

    async fn abort_multipart_upload(
        &self,
        req: S3Request<dto::AbortMultipartUploadInput>,
    ) -> S3Result<S3Response<dto::AbortMultipartUploadOutput>> {
        self.pending.lock().unwrap().remove(&req.input.upload_id);
        self.inner.abort_multipart_upload(req).await
    }

    async fn get_object_acl(
        &self,
        req: S3Request<dto::GetObjectAclInput>,
    ) -> S3Result<S3Response<dto::GetObjectAclOutput>> {
        let public = self
            .public
            .lock()
            .unwrap()
            .contains(&(req.input.bucket.clone(), req.input.key.clone()));
        let grants = if public {
            vec![dto::Grant {
                grantee: Some(dto::Grantee {
                    display_name: None,
                    email_address: None,
                    id: None,
                    type_: dto::Type::from_static(dto::Type::GROUP),
                    uri: Some(ALL_USERS.to_string()),
                }),
                permission: Some(dto::Permission::from_static(dto::Permission::READ)),
            }]
        } else {
            Vec::new()
        };
        Ok(S3Response::new(dto::GetObjectAclOutput {
            grants: Some(grants),
            owner: None,
            request_charged: None,
        }))
    }

    async fn put_object_acl(
        &self,
        req: S3Request<dto::PutObjectAclInput>,
    ) -> S3Result<S3Response<dto::PutObjectAclOutput>> {
        let public = Self::is_public_acl(req.input.acl.as_ref());
        self.set_public(&req.input.bucket, &req.input.key, public);
        Ok(S3Response::new(dto::PutObjectAclOutput {
            request_charged: None,
        }))
    }
}

// ---------------------------------------------------------------------------
// Server + client fixtures
// ---------------------------------------------------------------------------

/// Serve an S3 API over real TCP; returns the port.
async fn spawn_s3(root: &Path) -> u16 {
    let fs = s3s_fs::FileSystem::new(root).unwrap();
    let service = {
        let mut builder = S3ServiceBuilder::new(AclFs::new(fs));
        builder.set_auth(SimpleAuth::from_single(ACCESS_KEY, SECRET_KEY));
        builder.build()
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let service = service.clone();
            tokio::spawn(async move {
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(hyper_util::rt::TokioIo::new(stream), service)
                    .await;
            });
        }
    });
    port
}

fn fs_for(port: u16, bucket: Option<&str>) -> Arc<S3Fs> {
    S3Fs::new(S3Config {
        endpoint: format!("http://127.0.0.1:{port}"),
        region: "us-east-1".into(),
        access_key: ACCESS_KEY.into(),
        secret_key: Zeroizing::new(SECRET_KEY.into()),
        bucket: bucket.map(str::to_string),
        path_style: true,
        upload_acl: S3UploadAcl::Private,
    })
}

fn settings() -> TransferSettings {
    TransferSettings {
        max_parallel_per_server: 4,
        conflict_policy: ConflictPolicy::Overwrite,
        preserve_mtime: false,
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

fn statuses(entries: &[serverus_lib::session::s3::S3AclEntry]) -> HashMap<String, S3AclStatus> {
    entries.iter().map(|e| (e.path.clone(), e.status)).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn s3_bucket_level_operations() {
    let root = tempfile::tempdir().unwrap();
    let port = spawn_s3(root.path()).await;
    let fs = fs_for(port, None);

    fs.probe().await.unwrap();
    assert_eq!(fs.home_dir().await.unwrap(), "/");

    // mkdir at the root level creates a bucket.
    fs.mkdir("/first-bucket").await.unwrap();
    fs.mkdir("/second-bucket").await.unwrap();
    let roots = fs.list("/").await.unwrap();
    let names: Vec<&str> = roots.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"first-bucket"), "{names:?}");
    assert!(names.contains(&"second-bucket"), "{names:?}");
    assert!(roots.iter().all(|e| e.is_dir));

    // Objects and prefixes inside a bucket.
    fs.mkdir("/first-bucket/docs").await.unwrap();
    fs.create_file("/first-bucket/docs/a.txt").await.unwrap();
    assert!(fs.exists("/first-bucket/docs/a.txt").await.unwrap());
    assert!(fs.exists("/first-bucket/docs").await.unwrap()); // prefix = dir
    assert!(!fs.exists("/first-bucket/missing").await.unwrap());

    let listing = fs.list("/first-bucket/docs").await.unwrap();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].name, "a.txt");
    assert!(!listing[0].is_dir);

    // create_file refuses to overwrite.
    assert!(fs.create_file("/first-bucket/docs/a.txt").await.is_err());

    // Rename = copy + delete.
    fs.rename("/first-bucket/docs/a.txt", "/first-bucket/docs/b.txt")
        .await
        .unwrap();
    assert!(!fs.exists("/first-bucket/docs/a.txt").await.unwrap());
    assert!(fs.exists("/first-bucket/docs/b.txt").await.unwrap());

    // chmod is a clear error, mtime a silent no-op (SPEC §4.4).
    assert!(fs.chmod("/first-bucket/docs/b.txt", 0o644).await.is_err());
    fs.set_mtime("/first-bucket/docs/b.txt", 0).await.unwrap();

    // Recursive delete through the shared helper, then drop the bucket.
    delete_recursive(fs.as_ref(), "/first-bucket", true)
        .await
        .unwrap();
    let names: Vec<String> = fs
        .list("/")
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(!names.contains(&"first-bucket".to_string()), "{names:?}");
}

#[tokio::test]
async fn s3_recursive_directory_transfers() {
    let root = tempfile::tempdir().unwrap();
    let port = spawn_s3(root.path()).await;
    std::fs::create_dir(root.path().join("files")).unwrap();
    let fs = fs_for(port, Some("files"));
    fs.probe().await.unwrap();

    // Local tree with nesting — the founding pain point, S3 edition.
    let local = tempfile::tempdir().unwrap();
    let tree = local.path().join("site");
    std::fs::create_dir_all(tree.join("assets/img")).unwrap();
    std::fs::write(tree.join("index.html"), b"<html>hi</html>").unwrap();
    std::fs::write(tree.join("assets/app.js"), b"console.log(1)").unwrap();
    std::fs::write(tree.join("assets/img/logo.png"), vec![7u8; 1024]).unwrap();

    let manager = Arc::new(TransferManager::default());
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    manager
        .enqueue_upload(
            &sink,
            fs.clone(),
            "session",
            tree.to_str().unwrap(),
            "/",
            settings(),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);

    let listing = fs.list("/site/assets").await.unwrap();
    let names: Vec<&str> = listing.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"app.js"), "{names:?}");
    assert!(names.contains(&"img"), "{names:?}");

    // Round-trip: download the tree back and compare contents.
    let dl = tempfile::tempdir().unwrap();
    manager
        .enqueue_download(
            &sink,
            fs.clone(),
            "session",
            "/site",
            dl.path().to_str().unwrap(),
            settings(),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);

    assert_eq!(
        std::fs::read(dl.path().join("site/index.html")).unwrap(),
        b"<html>hi</html>"
    );
    assert_eq!(
        std::fs::read(dl.path().join("site/assets/img/logo.png")).unwrap(),
        vec![7u8; 1024]
    );

    // Remote recursive delete of the uploaded tree.
    delete_recursive(fs.as_ref(), "/site", true).await.unwrap();
    assert!(!fs.exists("/site").await.unwrap());
}

#[tokio::test]
async fn s3_multipart_upload_roundtrip() {
    let root = tempfile::tempdir().unwrap();
    let port = spawn_s3(root.path()).await;
    std::fs::create_dir(root.path().join("big")).unwrap();
    let fs = fs_for(port, Some("big"));

    // > 8 MiB part size → exercises create/upload_part/complete.
    let payload: Vec<u8> = (0..20 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    let local = tempfile::tempdir().unwrap();
    std::fs::write(local.path().join("blob.bin"), &payload).unwrap();

    let manager = Arc::new(TransferManager::default());
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    manager
        .enqueue_upload(
            &sink,
            fs.clone(),
            "session",
            local.path().join("blob.bin").to_str().unwrap(),
            "/",
            settings(),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);

    let entry = fs.stat("/blob.bin").await.unwrap();
    assert_eq!(entry.size, payload.len() as u64);

    let dl = tempfile::tempdir().unwrap();
    manager
        .enqueue_download(
            &sink,
            fs.clone(),
            "session",
            "/blob.bin",
            dl.path().to_str().unwrap(),
            settings(),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);
    assert_eq!(std::fs::read(dl.path().join("blob.bin")).unwrap(), payload);
}

#[tokio::test]
async fn s3_acl_public_private_flow() {
    let root = tempfile::tempdir().unwrap();
    let port = spawn_s3(root.path()).await;
    std::fs::create_dir(root.path().join("acl")).unwrap();
    let fs = fs_for(port, Some("acl"));

    // Uploads inherit the session's upload ACL (the pane switch).
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

    // Multipart uploads carry the ACL too (queue upload > part size).
    fs.set_upload_acl(S3UploadAcl::PublicRead);
    let payload = vec![3u8; 9 * 1024 * 1024];
    let local = tempfile::tempdir().unwrap();
    std::fs::write(local.path().join("big-public.bin"), &payload).unwrap();
    let manager = Arc::new(TransferManager::default());
    let sink: Arc<dyn ProgressSink> = Arc::new(NullSink);
    manager
        .enqueue_upload(
            &sink,
            fs.clone(),
            "session",
            local.path().join("big-public.bin").to_str().unwrap(),
            "/",
            settings(),
        )
        .await
        .unwrap();
    wait_for_drain(&manager).await;
    assert_all_done(&manager);
    let status = statuses(&fs.acl_status_batch(vec!["/big-public.bin".into()]).await);
    assert_eq!(status["/big-public.bin"], S3AclStatus::Public);

    // Bulk change: a directory applies recursively to every object.
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
    // Two files. (On real S3 the `docs/` placeholder object would be a
    // third; s3s-fs materialises it as a directory instead of an object.)
    assert_eq!(changed, 2);
    let status = statuses(
        &fs.acl_status_batch(vec!["/docs/a.txt".into(), "/docs/b.txt".into()])
            .await,
    );
    assert_eq!(status["/docs/a.txt"], S3AclStatus::Public);
    assert_eq!(status["/docs/b.txt"], S3AclStatus::Public);

    // And back to private for a single file.
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

    // Rename keeps the object public (ACL carry-over, SPEC §4.4).
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
