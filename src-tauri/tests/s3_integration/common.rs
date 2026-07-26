use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serverus_lib::session::s3::{S3AclEntry, S3AclStatus, S3Config, S3Fs};
use serverus_lib::transfer::{ProgressSink, TransferManager, TransferState};
use serverus_lib::vault::model::{ConflictPolicy, S3UploadAcl, TransferSettings};
use zeroize::Zeroizing;

pub(crate) const ACCESS_KEY: &str = "serverus-test-key";
pub(crate) const SECRET_KEY: &str = "serverus-test-secret";
pub(crate) const MULTIPART_PART_SIZE: usize = 8 * 1024 * 1024;

pub(crate) struct NullSink;

impl ProgressSink for NullSink {
    fn emit(&self, _event: serverus_lib::events::TransferProgressEvent) {}
}

pub(crate) fn fs_for(port: u16, bucket: Option<&str>) -> Arc<S3Fs> {
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

/// The `Content-Type` the server actually stored for an object, read with a
/// plain SDK client so the assertion does not go back through the code under
/// test.
pub(crate) async fn head_content_type(port: u16, bucket: &str, key: &str) -> Option<String> {
    let credentials = aws_sdk_s3::config::Credentials::new(
        ACCESS_KEY,
        SECRET_KEY,
        None,
        None,
        "serverus-content-type-test",
    );
    let config = aws_sdk_s3::Config::builder()
        .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
        .region(aws_sdk_s3::config::Region::new("us-east-1"))
        .endpoint_url(format!("http://127.0.0.1:{port}"))
        .credentials_provider(credentials)
        .force_path_style(true)
        .build();
    aws_sdk_s3::Client::from_conf(config)
        .head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .unwrap_or_else(|error| panic!("head {bucket}/{key}: {error}"))
        .content_type()
        .map(str::to_string)
}

pub(crate) fn settings() -> TransferSettings {
    TransferSettings {
        max_parallel_per_server: 4,
        conflict_policy: ConflictPolicy::Overwrite,
        preserve_mtime: false,
        tar_acceleration: false,
    }
}

pub(crate) async fn wait_for_drain(manager: &Arc<TransferManager>) {
    for _ in 0..600 {
        let summary = manager.snapshot().summary;
        if summary.queued == 0 && summary.running == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (items, summary) = {
        let s = manager.snapshot();
        (s.items, s.summary)
    };
    panic!("queue did not drain: {summary:?}\n{items:#?}");
}

pub(crate) fn assert_all_done(manager: &Arc<TransferManager>) {
    let items = manager.snapshot().items;
    for item in &items {
        assert!(
            matches!(item.state, TransferState::Done),
            "item not done: {item:#?}"
        );
    }
}

pub(crate) fn statuses(entries: &[S3AclEntry]) -> HashMap<String, S3AclStatus> {
    entries
        .iter()
        .map(|entry| (entry.path.clone(), entry.status))
        .collect()
}
