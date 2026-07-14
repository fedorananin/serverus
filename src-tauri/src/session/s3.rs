//! S3-compatible object storage implementation of [`RemoteFs`] (SPEC §4.4).
//!
//! Works against any S3-compatible endpoint (AWS S3, DigitalOcean Spaces,
//! Cloudflare R2, Backblaze B2, Wasabi, MinIO, …). "Directories" are key
//! prefixes with a `/` delimiter; `mkdir` creates the conventional zero-byte
//! `dir/` placeholder object. When the connection has no fixed bucket, the
//! panel root lists all buckets as folders (mkdir/rmdir there manages
//! buckets).
//!
//! ACLs (the public/private feature) are deliberately NOT part of the
//! `RemoteFs` trait — the UI reaches them through dedicated `s3_*` commands
//! so the protocol abstraction stays intact.

use std::pin::Pin;
use std::sync::{Arc, Mutex, RwLock};
use std::task::{Context, Poll};

use aws_sdk_s3::config::{BehaviorVersion, Credentials, Region};
use aws_sdk_s3::error::{ProvideErrorMetadata, SdkError};
use aws_sdk_s3::types::{
    BucketLocationConstraint, CompletedMultipartUpload, CompletedPart, CreateBucketConfiguration,
    ObjectCannedAcl, Permission,
};
use aws_sdk_s3::Client;
use futures::future::BoxFuture;
use futures::{FutureExt, StreamExt};
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::io::AsyncWrite;
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};
use crate::session::remote_fs::{join_remote, BoxRead, BoxWrite, RemoteEntry, RemoteFs};
use crate::session::LifecycleCleanup;
use crate::vault::model::{Connection, S3UploadAcl};

/// Multipart part size. Must be ≥ 5 MiB (S3 minimum for non-final parts).
const PART_SIZE: usize = 8 * 1024 * 1024;
/// Parallelism for per-object ACL requests (batch status + bulk set).
const ACL_CONCURRENCY: usize = 8;
/// Grantee URI meaning "everyone" — its READ grant is what "public" means.
const ALL_USERS_URI: &str = "http://acs.amazonaws.com/groups/global/AllUsers";

pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub access_key: String,
    pub secret_key: Zeroizing<String>,
    pub bucket: Option<String>,
    pub path_style: bool,
    pub upload_acl: S3UploadAcl,
}

impl S3Config {
    pub fn from_connection(conn: &Connection) -> AppResult<Self> {
        let opts = conn.s3.clone().unwrap_or_default();
        let host = conn.host.trim();
        if host.is_empty() {
            return Err(AppError::Connect("S3 endpoint is empty".into()));
        }
        // `host` is an endpoint hostname; a full URL is accepted as-is.
        let endpoint = if host.contains("://") {
            host.to_string()
        } else if conn.port == 443 || conn.port == 0 {
            format!("https://{host}")
        } else {
            format!("https://{host}:{}", conn.port)
        };
        Ok(S3Config {
            endpoint,
            region: opts
                .region
                .filter(|r| !r.trim().is_empty())
                .unwrap_or_else(|| "us-east-1".into()),
            access_key: conn.auth.username.clone(),
            secret_key: Zeroizing::new(conn.auth.password.clone().unwrap_or_default()),
            bucket: opts.bucket.filter(|b| !b.trim().is_empty()),
            path_style: opts.path_style,
            upload_acl: opts.upload_acl,
        })
    }
}

/// Where a panel path points to in S3 terms.
enum Loc {
    /// Panel root with no fixed bucket — the bucket list.
    Root,
    /// A bucket itself (its root prefix).
    Bucket(String),
    /// An object or prefix inside a bucket. The key has no trailing slash.
    Key(String, String),
}

pub struct S3Fs {
    client: Client,
    /// Fixed bucket from the connection config; `None` = bucket list at root.
    bucket: Option<String>,
    region: String,
    /// ACL for uploads; runtime-switchable via the pane toggle. `Ask` acts
    /// like `Private` — the UI resolves the answer before enqueueing.
    upload_acl: RwLock<S3UploadAcl>,
    cleanup: Option<LifecycleCleanup>,
}

fn sdk_err<E>(op: &str, e: SdkError<E, aws_sdk_s3::config::http::HttpResponse>) -> AppError
where
    E: ProvideErrorMetadata + std::error::Error + Send + Sync + 'static,
{
    AppError::RemoteFs(format!("{op}: {}", sdk_err_msg(&e)))
}

fn sdk_err_msg<E>(e: &SdkError<E, aws_sdk_s3::config::http::HttpResponse>) -> String
where
    E: ProvideErrorMetadata + std::error::Error + Send + Sync + 'static,
{
    match e {
        SdkError::ServiceError(se) => {
            let meta = se.err().meta();
            match (meta.code(), meta.message()) {
                (Some(code), Some(msg)) => format!("{code}: {msg}"),
                (Some(code), None) => code.to_string(),
                (None, Some(msg)) => msg.to_string(),
                (None, None) => "service error".into(),
            }
        }
        // Dispatch/timeout errors carry the useful part in their source.
        other => match std::error::Error::source(other) {
            Some(src) => format!("{other}: {src}"),
            None => other.to_string(),
        },
    }
}

fn is_not_found<E>(e: &SdkError<E, aws_sdk_s3::config::http::HttpResponse>) -> bool
where
    E: ProvideErrorMetadata + std::error::Error + Send + Sync + 'static,
{
    if let SdkError::ServiceError(se) = e {
        if se.raw().status().as_u16() == 404 {
            return true;
        }
        matches!(se.err().meta().code(), Some("NoSuchKey" | "NotFound"))
    } else {
        false
    }
}

/// Percent-encode an object key for use in `x-amz-copy-source` (slashes kept).
fn encode_copy_source(bucket: &str, key: &str) -> String {
    const KEEP: percent_encoding::AsciiSet = percent_encoding::NON_ALPHANUMERIC
        .remove(b'/')
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    format!(
        "{bucket}/{}",
        percent_encoding::utf8_percent_encode(key, &KEEP)
    )
}

// ---------------------------------------------------------------------------
// ACL DTOs (cross the IPC boundary via the s3_* commands)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum S3AclStatus {
    Public,
    Private,
    /// The provider rejected GetObjectAcl (e.g. R2) or the request failed.
    Unknown,
}

#[derive(Debug, Clone, Serialize, Type)]
pub struct S3AclEntry {
    pub path: String,
    pub status: S3AclStatus,
}

#[derive(Debug, Clone, Deserialize, Type)]
pub struct S3AclTarget {
    pub path: String,
    pub is_dir: bool,
}

impl S3Fs {
    pub fn new(config: S3Config) -> Arc<S3Fs> {
        Self::new_inner(config, None)
    }

    pub(crate) fn new_in_lifecycle(config: S3Config, cleanup: LifecycleCleanup) -> Arc<S3Fs> {
        Self::new_inner(config, Some(cleanup))
    }

    fn new_inner(config: S3Config, cleanup: Option<LifecycleCleanup>) -> Arc<S3Fs> {
        let credentials = Credentials::new(
            config.access_key.clone(),
            config.secret_key.to_string(),
            None,
            None,
            "serverus-vault",
        );
        let sdk_config =
            aws_sdk_s3::Config::builder()
                .behavior_version(BehaviorVersion::latest())
                .region(Region::new(config.region.clone()))
                .endpoint_url(&config.endpoint)
                .credentials_provider(credentials)
                .force_path_style(config.path_style)
                // Transfers can sit paused for a long time; the stalled-stream
                // guard would abort them.
                .stalled_stream_protection(
                    aws_sdk_s3::config::StalledStreamProtectionConfig::disabled(),
                )
                .build();
        Arc::new(S3Fs {
            client: Client::from_conf(sdk_config),
            bucket: config.bucket,
            region: config.region,
            upload_acl: RwLock::new(config.upload_acl),
            cleanup,
        })
    }

    /// Verify credentials/reachability once at session-connect time.
    pub async fn probe(&self) -> AppResult<()> {
        match &self.bucket {
            Some(bucket) => self
                .client
                .head_bucket()
                .bucket(bucket)
                .send()
                .await
                .map(|_| ())
                .map_err(|e| AppError::Connect(format!("bucket {bucket}: {}", sdk_err_msg(&e)))),
            None => self
                .client
                .list_buckets()
                .send()
                .await
                .map(|_| ())
                .map_err(|e| AppError::Connect(format!("list buckets: {}", sdk_err_msg(&e)))),
        }
    }

    pub fn set_upload_acl(&self, mode: S3UploadAcl) {
        *self.upload_acl.write().unwrap() = mode;
    }

    /// The canned ACL applied to uploaded objects. `Private`/`Ask` send no
    /// ACL header at all — the provider default is private, and ACL-less
    /// providers (R2) reject any explicit ACL.
    fn upload_canned_acl(&self) -> Option<ObjectCannedAcl> {
        match *self.upload_acl.read().unwrap() {
            S3UploadAcl::PublicRead => Some(ObjectCannedAcl::PublicRead),
            S3UploadAcl::Private | S3UploadAcl::Ask => None,
        }
    }

    fn resolve(&self, path: &str) -> AppResult<Loc> {
        let rel = path.trim_matches('/');
        match (&self.bucket, rel.is_empty()) {
            (Some(b), true) => Ok(Loc::Bucket(b.clone())),
            (Some(b), false) => Ok(Loc::Key(b.clone(), rel.to_string())),
            (None, true) => Ok(Loc::Root),
            (None, false) => match rel.split_once('/') {
                Some((bucket, key)) if !key.is_empty() => {
                    Ok(Loc::Key(bucket.to_string(), key.to_string()))
                }
                _ => Ok(Loc::Bucket(rel.to_string())),
            },
        }
    }

    /// (bucket, key) for paths that must name an object.
    fn object(&self, path: &str) -> AppResult<(String, String)> {
        match self.resolve(path)? {
            Loc::Key(b, k) => Ok((b, k)),
            _ => Err(AppError::RemoteFs(format!("{path}: not an object path"))),
        }
    }

    /// (bucket, prefix) for paths denoting a "directory": bucket root → "",
    /// nested → "key/".
    fn dir_prefix(&self, path: &str) -> AppResult<(String, String)> {
        match self.resolve(path)? {
            Loc::Bucket(b) => Ok((b, String::new())),
            Loc::Key(b, k) => Ok((b, format!("{k}/"))),
            Loc::Root => Err(AppError::RemoteFs("not inside a bucket".into())),
        }
    }

    async fn list_buckets(&self) -> AppResult<Vec<RemoteEntry>> {
        let out = self
            .client
            .list_buckets()
            .send()
            .await
            .map_err(|e| sdk_err("list buckets", e))?;
        Ok(out
            .buckets()
            .iter()
            .filter_map(|b| {
                let name = b.name()?.to_string();
                Some(RemoteEntry {
                    path: format!("/{name}"),
                    is_dir: true,
                    is_symlink: false,
                    size: 0,
                    mtime: b.creation_date().map(|d| d.secs()),
                    permissions: None,
                    name,
                })
            })
            .collect())
    }

    /// All object keys under a prefix (no delimiter — full recursive set).
    async fn list_all_keys(&self, bucket: &str, prefix: &str) -> AppResult<Vec<String>> {
        let mut keys = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut req = self.client.list_objects_v2().bucket(bucket);
            if !prefix.is_empty() {
                req = req.prefix(prefix);
            }
            if let Some(t) = token.take() {
                req = req.continuation_token(t);
            }
            let out = req.send().await.map_err(|e| sdk_err(prefix, e))?;
            keys.extend(
                out.contents()
                    .iter()
                    .filter_map(|o| o.key().map(str::to_string)),
            );
            match out.next_continuation_token() {
                Some(t) if out.is_truncated() == Some(true) => token = Some(t.to_string()),
                _ => break,
            }
        }
        Ok(keys)
    }

    async fn acl_status_one(&self, bucket: &str, key: &str) -> S3AclStatus {
        match self
            .client
            .get_object_acl()
            .bucket(bucket)
            .key(key)
            .send()
            .await
        {
            Ok(out) => {
                let public = out.grants().iter().any(|g| {
                    g.grantee().and_then(|gr| gr.uri()) == Some(ALL_USERS_URI)
                        && matches!(
                            g.permission(),
                            Some(Permission::Read | Permission::FullControl)
                        )
                });
                if public {
                    S3AclStatus::Public
                } else {
                    S3AclStatus::Private
                }
            }
            Err(_) => S3AclStatus::Unknown,
        }
    }

    /// Batch public/private status for the panel badges. Never fails — files
    /// whose ACL cannot be read come back as `Unknown`.
    pub async fn acl_status_batch(&self, paths: Vec<String>) -> Vec<S3AclEntry> {
        futures::stream::iter(paths)
            .map(|path| async move {
                let status = match self.object(&path) {
                    Ok((bucket, key)) => self.acl_status_one(&bucket, &key).await,
                    Err(_) => S3AclStatus::Unknown,
                };
                S3AclEntry { path, status }
            })
            .buffer_unordered(ACL_CONCURRENCY)
            .collect()
            .await
    }

    async fn put_acl(&self, bucket: &str, key: &str, public: bool) -> AppResult<()> {
        let acl = if public {
            ObjectCannedAcl::PublicRead
        } else {
            ObjectCannedAcl::Private
        };
        self.client
            .put_object_acl()
            .bucket(bucket)
            .key(key)
            .acl(acl)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| {
                let msg = sdk_err_msg(&e);
                if matches!(
                    e.into_service_error().meta().code(),
                    Some("NotImplemented" | "AccessControlListNotSupported" | "InvalidRequest")
                ) {
                    AppError::RemoteFs(format!(
                        "{key}: this provider does not support per-object ACLs \
                         (public access is configured at the bucket level): {msg}"
                    ))
                } else {
                    AppError::RemoteFs(format!("{key}: {msg}"))
                }
            })
    }

    /// Make objects public or private. Directories apply to every object
    /// under the prefix. Returns the number of objects changed.
    pub async fn set_acl(&self, targets: Vec<S3AclTarget>, public: bool) -> AppResult<u32> {
        // Expand directories into their full recursive key sets first.
        let mut objects: Vec<(String, String)> = Vec::new();
        for target in &targets {
            if target.is_dir {
                let (bucket, prefix) = self.dir_prefix(&target.path)?;
                for key in self.list_all_keys(&bucket, &prefix).await? {
                    objects.push((bucket.clone(), key));
                }
            } else {
                objects.push(self.object(&target.path)?);
            }
        }
        let total = objects.len() as u32;
        let mut stream = futures::stream::iter(objects)
            .map(|(bucket, key)| async move { self.put_acl(&bucket, &key, public).await })
            .buffer_unordered(ACL_CONCURRENCY);
        while let Some(result) = stream.next().await {
            result?;
        }
        Ok(total)
    }

    /// Best-effort ACL carry-over for rename (CopyObject resets the ACL).
    async fn preserve_acl(&self, from: (&str, &str), to: (&str, &str)) {
        if self.acl_status_one(from.0, from.1).await == S3AclStatus::Public {
            let _ = self.put_acl(to.0, to.1, true).await;
        }
    }

    async fn copy_object(&self, from: (&str, &str), to: (&str, &str)) -> AppResult<()> {
        self.client
            .copy_object()
            .copy_source(encode_copy_source(from.0, from.1))
            .bucket(to.0)
            .key(to.1)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| sdk_err(from.1, e))?;
        self.preserve_acl(from, to).await;
        Ok(())
    }

    async fn delete_object(&self, bucket: &str, key: &str) -> AppResult<()> {
        self.client
            .delete_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map(|_| ())
            .map_err(|e| sdk_err(key, e))
    }

    async fn head(&self, bucket: &str, key: &str) -> AppResult<Option<RemoteEntry>> {
        match self
            .client
            .head_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
        {
            Ok(out) => Ok(Some(RemoteEntry {
                name: key.rsplit('/').next().unwrap_or(key).to_string(),
                path: self.display_path(bucket, key),
                is_dir: false,
                is_symlink: false,
                size: out.content_length().unwrap_or(0).max(0) as u64,
                mtime: out.last_modified().map(|d| d.secs()),
                permissions: None,
            })),
            Err(e) if is_not_found(&e) => Ok(None),
            Err(e) => Err(sdk_err(key, e)),
        }
    }

    /// Whether any object exists under `prefix` (the "directory exists" test).
    async fn prefix_exists(&self, bucket: &str, prefix: &str) -> AppResult<bool> {
        let out = self
            .client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(prefix)
            .max_keys(1)
            .send()
            .await
            .map_err(|e| sdk_err(prefix, e))?;
        Ok(out.key_count().unwrap_or(0) > 0)
    }

    fn display_path(&self, bucket: &str, key: &str) -> String {
        match &self.bucket {
            Some(_) => format!("/{key}"),
            None => format!("/{bucket}/{key}"),
        }
    }

    fn dir_entry(name: &str, path: &str) -> RemoteEntry {
        RemoteEntry {
            name: name.to_string(),
            path: path.to_string(),
            is_dir: true,
            is_symlink: false,
            size: 0,
            mtime: None,
            permissions: None,
        }
    }
}

#[async_trait::async_trait]
impl RemoteFs for S3Fs {
    async fn list(&self, path: &str) -> AppResult<Vec<RemoteEntry>> {
        let (bucket, prefix) = match self.resolve(path)? {
            Loc::Root => return self.list_buckets().await,
            Loc::Bucket(b) => (b, String::new()),
            Loc::Key(b, k) => (b, format!("{k}/")),
        };
        let mut entries = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut req = self.client.list_objects_v2().bucket(&bucket).delimiter("/");
            if !prefix.is_empty() {
                req = req.prefix(&prefix);
            }
            if let Some(t) = token.take() {
                req = req.continuation_token(t);
            }
            let out = req.send().await.map_err(|e| sdk_err(path, e))?;
            for cp in out.common_prefixes() {
                let Some(full) = cp.prefix() else { continue };
                let name = full
                    .trim_end_matches('/')
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .to_string();
                if name.is_empty() {
                    continue;
                }
                entries.push(RemoteEntry {
                    path: join_remote(if path.is_empty() { "/" } else { path }, &name),
                    ..Self::dir_entry(&name, "")
                });
            }
            for obj in out.contents() {
                let Some(key) = obj.key() else { continue };
                // Skip the listed directory's own placeholder object.
                if key == prefix {
                    continue;
                }
                let name = key.rsplit('/').next().unwrap_or(key).to_string();
                if name.is_empty() {
                    continue;
                }
                entries.push(RemoteEntry {
                    name: name.clone(),
                    path: self.display_path(&bucket, key),
                    is_dir: false,
                    is_symlink: false,
                    size: obj.size().unwrap_or(0).max(0) as u64,
                    mtime: obj.last_modified().map(|d| d.secs()),
                    permissions: None,
                });
            }
            match out.next_continuation_token() {
                Some(t) if out.is_truncated() == Some(true) => token = Some(t.to_string()),
                _ => break,
            }
        }
        Ok(entries)
    }

    async fn stat(&self, path: &str) -> AppResult<RemoteEntry> {
        match self.resolve(path)? {
            Loc::Root => Ok(Self::dir_entry("/", "/")),
            Loc::Bucket(b) => Ok(Self::dir_entry(&b, path)),
            Loc::Key(bucket, key) => {
                // Some providers answer HeadObject on a directory-like key
                // with odd errors instead of 404 — always fall through to
                // the prefix probe before giving up.
                let head = self.head(&bucket, &key).await;
                if let Ok(Some(entry)) = head {
                    return Ok(entry);
                }
                if self.prefix_exists(&bucket, &format!("{key}/")).await? {
                    let name = key.rsplit('/').next().unwrap_or(&key);
                    return Ok(Self::dir_entry(name, &self.display_path(&bucket, &key)));
                }
                match head {
                    Err(e) => Err(e),
                    Ok(_) => Err(AppError::RemoteFs(format!("{path}: not found"))),
                }
            }
        }
    }

    async fn home_dir(&self) -> AppResult<String> {
        Ok("/".into())
    }

    async fn mkdir(&self, path: &str) -> AppResult<()> {
        match self.resolve(path)? {
            Loc::Root => Err(AppError::RemoteFs("cannot create '/'".into())),
            // Bucket level (no fixed bucket): mkdir = create bucket.
            Loc::Bucket(bucket) => {
                let mut req = self.client.create_bucket().bucket(&bucket);
                if self.region != "us-east-1" {
                    req = req.create_bucket_configuration(
                        CreateBucketConfiguration::builder()
                            .location_constraint(BucketLocationConstraint::from(
                                self.region.as_str(),
                            ))
                            .build(),
                    );
                }
                req.send()
                    .await
                    .map(|_| ())
                    .map_err(|e| sdk_err(&bucket, e))
            }
            // Directory placeholder: the `dir/` convention.
            Loc::Key(bucket, key) => self
                .client
                .put_object()
                .bucket(&bucket)
                .key(format!("{key}/"))
                .body(Vec::new().into())
                .send()
                .await
                .map(|_| ())
                .map_err(|e| sdk_err(path, e)),
        }
    }

    async fn create_file(&self, path: &str) -> AppResult<()> {
        let (bucket, key) = self.object(path)?;
        if self.head(&bucket, &key).await?.is_some() {
            return Err(AppError::RemoteFs(format!("{path}: already exists")));
        }
        let mut req = self
            .client
            .put_object()
            .bucket(&bucket)
            .key(&key)
            .body(Vec::new().into());
        if let Some(acl) = self.upload_canned_acl() {
            req = req.acl(acl);
        }
        req.send().await.map(|_| ()).map_err(|e| sdk_err(path, e))
    }

    async fn rename(&self, from: &str, to: &str) -> AppResult<()> {
        let (fb, fk) = self.object(from)?;
        let (tb, tk) = self.object(to)?;
        // Plain object: copy + delete.
        if self.head(&fb, &fk).await?.is_some() {
            self.copy_object((&fb, &fk), (&tb, &tk)).await?;
            return self.delete_object(&fb, &fk).await;
        }
        // Directory: move every object under the prefix.
        let from_prefix = format!("{fk}/");
        let keys = self.list_all_keys(&fb, &from_prefix).await?;
        if keys.is_empty() {
            return Err(AppError::RemoteFs(format!("{from}: not found")));
        }
        for key in keys {
            let suffix = &key[from_prefix.len()..];
            let target = format!("{tk}/{suffix}");
            self.copy_object((&fb, &key), (&tb, &target)).await?;
            self.delete_object(&fb, &key).await?;
        }
        Ok(())
    }

    async fn replace_file(&self, staged: &str, target: &str) -> AppResult<()> {
        let (source_bucket, source_key) = self.object(staged)?;
        let (target_bucket, target_key) = self.object(target)?;
        let target_acl = self.acl_status_one(&target_bucket, &target_key).await;

        // CopyObject publishes only the completed staged object at the
        // destination. Apply the previous destination ACL in the same request
        // instead of inheriting the private staging object's visibility.
        let mut request = self
            .client
            .copy_object()
            .copy_source(encode_copy_source(&source_bucket, &source_key))
            .bucket(&target_bucket)
            .key(&target_key);
        request = match target_acl {
            S3AclStatus::Public => request.acl(ObjectCannedAcl::PublicRead),
            // The provider default is private. Omitting the header also keeps
            // replacement compatible with ACL-less providers.
            S3AclStatus::Private | S3AclStatus::Unknown => request,
        };
        request
            .send()
            .await
            .map_err(|error| sdk_err(staged, error))?;

        // The destination is complete and visible before staging cleanup.
        self.delete_object(&source_bucket, &source_key).await
    }

    async fn delete_file(&self, path: &str) -> AppResult<()> {
        let (bucket, key) = self.object(path)?;
        self.delete_object(&bucket, &key).await
    }

    async fn delete_dir(&self, path: &str) -> AppResult<()> {
        match self.resolve(path)? {
            Loc::Root => Err(AppError::RemoteFs("cannot delete '/'".into())),
            // Bucket level: recursive delete above has emptied it.
            Loc::Bucket(bucket) => self
                .client
                .delete_bucket()
                .bucket(&bucket)
                .send()
                .await
                .map(|_| ())
                .map_err(|e| sdk_err(&bucket, e)),
            // Deleting a missing placeholder is fine — prefixes with no
            // marker object "exist" only through their children.
            Loc::Key(bucket, key) => self.delete_object(&bucket, &format!("{key}/")).await,
        }
    }

    async fn chmod(&self, _path: &str, _mode: u32) -> AppResult<()> {
        Err(AppError::RemoteFs(
            "S3 objects have no POSIX permissions — use Make public / Make private".into(),
        ))
    }

    async fn set_mtime(&self, _path: &str, _mtime_unix: i64) -> AppResult<()> {
        // S3 has no settable mtime; LastModified is server-managed.
        Ok(())
    }

    async fn open_read(&self, path: &str, offset: u64) -> AppResult<BoxRead> {
        let (bucket, key) = self.object(path)?;
        let mut req = self.client.get_object().bucket(&bucket).key(&key);
        if offset > 0 {
            req = req.range(format!("bytes={offset}-"));
        }
        let out = req.send().await.map_err(|e| sdk_err(path, e))?;
        Ok(Box::new(out.body.into_async_read()))
    }

    async fn open_write(&self, path: &str, offset: u64) -> AppResult<BoxWrite> {
        if offset > 0 {
            return Err(AppError::RemoteFs(
                "S3 uploads cannot resume — retry restarts the file".into(),
            ));
        }
        let (bucket, key) = self.object(path)?;
        Ok(Box::new(S3Writer::new(
            self.client.clone(),
            bucket,
            key,
            self.upload_canned_acl(),
            self.cleanup.clone(),
        )))
    }

    async fn open_write_replacement(&self, staged: &str, _target: &str) -> AppResult<BoxWrite> {
        let (bucket, key) = self.object(staged)?;
        // Remote-edit staging may contain sensitive data and must never use a
        // session-wide PublicRead policy. Omitting the ACL keeps the provider
        // default private and works with providers that reject ACL headers.
        Ok(Box::new(S3Writer::new(
            self.client.clone(),
            bucket,
            key,
            None,
            self.cleanup.clone(),
        )))
    }

    async fn exists(&self, path: &str) -> AppResult<bool> {
        match self.stat(path).await {
            Ok(_) => Ok(true),
            Err(AppError::RemoteFs(msg)) if msg.contains("not found") => Ok(false),
            Err(e) => Err(e),
        }
    }

    fn supports_write_resume(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Streaming upload: buffered multipart with a plain PutObject fast path
// ---------------------------------------------------------------------------

struct WriterInner {
    client: Client,
    bucket: String,
    key: String,
    acl: Option<ObjectCannedAcl>,
    abort: Arc<AbortSlot>,
    upload_id: Option<String>,
    parts: Vec<CompletedPart>,
    next_part: i32,
    buf: Vec<u8>,
}

impl WriterInner {
    /// Upload the buffered bytes as the next part (creates the multipart
    /// upload lazily on the first call).
    async fn flush_part(mut self: Box<Self>) -> std::io::Result<Box<Self>> {
        if self.upload_id.is_none() {
            let mut req = self
                .client
                .create_multipart_upload()
                .bucket(&self.bucket)
                .key(&self.key);
            if let Some(acl) = &self.acl {
                req = req.acl(acl.clone());
            }
            let out = req
                .send()
                .await
                .map_err(|e| std::io::Error::other(sdk_err_msg(&e)))?;
            let upload_id = out
                .upload_id()
                .ok_or_else(|| std::io::Error::other("no upload id"))?
                .to_string();
            *self.abort.upload_id.lock().unwrap() = Some(upload_id.clone());
            self.upload_id = Some(upload_id);
        }
        let body = std::mem::take(&mut self.buf);
        let part_number = self.next_part;
        self.next_part += 1;
        let out = self
            .client
            .upload_part()
            .bucket(&self.bucket)
            .key(&self.key)
            .upload_id(self.upload_id.as_deref().unwrap_or_default())
            .part_number(part_number)
            .body(body.into())
            .send()
            .await
            .map_err(|e| std::io::Error::other(sdk_err_msg(&e)))?;
        self.parts.push(
            CompletedPart::builder()
                .part_number(part_number)
                .set_e_tag(out.e_tag().map(str::to_string))
                .build(),
        );
        Ok(self)
    }

    async fn finish(mut self: Box<Self>) -> std::io::Result<()> {
        match self.upload_id.clone() {
            // Small object: everything still buffered — one PutObject.
            None => {
                let mut req = self
                    .client
                    .put_object()
                    .bucket(&self.bucket)
                    .key(&self.key)
                    .body(std::mem::take(&mut self.buf).into());
                if let Some(acl) = &self.acl {
                    req = req.acl(acl.clone());
                }
                req.send()
                    .await
                    .map(|_| ())
                    .map_err(|e| std::io::Error::other(sdk_err_msg(&e)))
            }
            Some(upload_id) => {
                if !self.buf.is_empty() {
                    self = self.flush_part().await?;
                }
                self.client
                    .complete_multipart_upload()
                    .bucket(&self.bucket)
                    .key(&self.key)
                    .upload_id(&upload_id)
                    .multipart_upload(
                        CompletedMultipartUpload::builder()
                            .set_parts(Some(std::mem::take(&mut self.parts)))
                            .build(),
                    )
                    .send()
                    .await
                    .map(|_| ())
                    .map_err(|e| std::io::Error::other(sdk_err_msg(&e)))
            }
        }
    }
}

enum WriterState {
    Ready(Box<WriterInner>),
    /// A part upload in flight; the inner state comes back when it lands.
    Busy(BoxFuture<'static, std::io::Result<Box<WriterInner>>>),
    Finishing(BoxFuture<'static, std::io::Result<()>>),
    Done,
    Failed,
}

/// Info needed to abort an incomplete multipart upload if the writer is
/// dropped mid-transfer (cancel / error) — otherwise orphaned parts linger
/// (and bill) on the provider.
struct AbortSlot {
    client: Client,
    bucket: String,
    key: String,
    upload_id: Mutex<Option<String>>,
}

pub struct S3Writer {
    state: WriterState,
    abort: Arc<AbortSlot>,
    cleanup: Option<LifecycleCleanup>,
}

impl S3Writer {
    fn new(
        client: Client,
        bucket: String,
        key: String,
        acl: Option<ObjectCannedAcl>,
        cleanup: Option<LifecycleCleanup>,
    ) -> S3Writer {
        let abort = Arc::new(AbortSlot {
            client: client.clone(),
            bucket: bucket.clone(),
            key: key.clone(),
            upload_id: Mutex::new(None),
        });
        S3Writer {
            state: WriterState::Ready(Box::new(WriterInner {
                client,
                bucket,
                key,
                acl,
                abort: abort.clone(),
                upload_id: None,
                parts: Vec::new(),
                next_part: 1,
                buf: Vec::new(),
            })),
            abort,
            cleanup,
        }
    }

    /// Drive a Busy state to completion; returns Pending while in flight.
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        loop {
            match &mut self.state {
                WriterState::Ready(_) | WriterState::Done => return Poll::Ready(Ok(())),
                WriterState::Busy(fut) => match futures::ready!(fut.as_mut().poll(cx)) {
                    Ok(inner) => {
                        *self.abort.upload_id.lock().unwrap() = inner.upload_id.clone();
                        self.state = WriterState::Ready(inner);
                    }
                    Err(e) => {
                        self.state = WriterState::Failed;
                        return Poll::Ready(Err(e));
                    }
                },
                WriterState::Finishing(_) => {
                    return Poll::Ready(Err(std::io::Error::other("write after shutdown")))
                }
                WriterState::Failed => {
                    return Poll::Ready(Err(std::io::Error::other("upload already failed")))
                }
            }
        }
    }
}

impl AsyncWrite for S3Writer {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        futures::ready!(self.poll_ready(cx))?;
        let WriterState::Ready(inner) = &mut self.state else {
            return Poll::Ready(Err(std::io::Error::other("write after shutdown")));
        };
        inner.buf.extend_from_slice(buf);
        if inner.buf.len() >= PART_SIZE {
            let WriterState::Ready(inner) = std::mem::replace(&mut self.state, WriterState::Done)
            else {
                unreachable!()
            };
            self.state = WriterState::Busy(inner.flush_part().boxed());
            // Kick the upload off; the bytes are accepted either way and the
            // next poll_* call continues driving it.
            let _ = self.poll_ready(cx)?;
        }
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        // Buffered bytes below the part threshold can only go out on
        // shutdown; "flush" just drains any in-flight part.
        self.poll_ready(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        loop {
            match &mut self.state {
                WriterState::Ready(_) => {
                    let WriterState::Ready(inner) =
                        std::mem::replace(&mut self.state, WriterState::Done)
                    else {
                        unreachable!()
                    };
                    self.state = WriterState::Finishing(inner.finish().boxed());
                }
                WriterState::Busy(_) => futures::ready!(self.poll_ready(cx))?,
                WriterState::Finishing(fut) => {
                    let result = futures::ready!(fut.as_mut().poll(cx));
                    self.state = if result.is_ok() {
                        *self.abort.upload_id.lock().unwrap() = None;
                        WriterState::Done
                    } else {
                        WriterState::Failed
                    };
                    return Poll::Ready(result);
                }
                WriterState::Done => return Poll::Ready(Ok(())),
                WriterState::Failed => {
                    return Poll::Ready(Err(std::io::Error::other("upload already failed")))
                }
            }
        }
    }
}

impl Drop for S3Writer {
    fn drop(&mut self) {
        // Cancel any in-flight part/completion request before starting the
        // abort request for the same multipart upload.
        drop(std::mem::replace(&mut self.state, WriterState::Done));
        // A multipart upload that never completed leaves billable orphaned
        // parts — abort it in the background.
        let upload_id = self.abort.upload_id.lock().unwrap().take();
        if let Some(upload_id) = upload_id {
            let slot = self.abort.clone();
            let abort = async move {
                let request = slot
                    .client
                    .abort_multipart_upload()
                    .bucket(&slot.bucket)
                    .key(&slot.key)
                    .upload_id(upload_id)
                    .send();
                let _ = tokio::time::timeout(std::time::Duration::from_secs(5), request).await;
            };
            let abort = if let Some(cleanup) = &self.cleanup {
                match cleanup.try_spawn(abort) {
                    Ok(()) => return,
                    Err(abort) => abort,
                }
            } else {
                abort
            };
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                runtime.spawn(abort);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::{S3Config, S3Fs, S3Writer};
    use crate::session::lifecycle::LifecycleGate;
    use crate::vault::model::S3UploadAcl;

    #[tokio::test]
    async fn multipart_abort_stays_in_the_session_close_barrier() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (request_started, request_started_rx) = tokio::sync::oneshot::channel();
        let (release_response, release_response_rx) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = socket.read(&mut buffer).await.unwrap();
                assert_ne!(read, 0, "client closed before sending abort request");
                request.extend_from_slice(&buffer[..read]);
            }
            let request = String::from_utf8_lossy(&request);
            assert!(request.starts_with("DELETE "), "{request}");
            assert!(request.contains("uploadId=upload-id"), "{request}");
            let _ = request_started.send(());
            let _ = release_response_rx.await;
            socket
                .write_all(
                    b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await
                .unwrap();
        });

        let lifecycle = Arc::new(LifecycleGate::default());
        let operation = lifecycle.try_begin_operation().unwrap();
        let fs = S3Fs::new_in_lifecycle(
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
        let writer = S3Writer::new(
            fs.client.clone(),
            "bucket".into(),
            "key".into(),
            None,
            fs.cleanup.clone(),
        );
        *writer.abort.upload_id.lock().unwrap() = Some("upload-id".into());

        let close = tokio::spawn({
            let lifecycle = lifecycle.clone();
            async move { lifecycle.begin_close().await }
        });
        operation.cancelled().await;
        drop(writer);
        drop(operation);
        tokio::time::timeout(Duration::from_secs(1), request_started_rx)
            .await
            .expect("multipart abort request did not start")
            .unwrap();
        assert!(!close.is_finished());

        release_response.send(()).unwrap();
        let guard = tokio::time::timeout(Duration::from_secs(1), close)
            .await
            .expect("session close did not wait for multipart abort")
            .unwrap();
        guard.finish().await;
        server.await.unwrap();
    }
}
