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
        let (_, summary) = manager.snapshot();
        if summary.queued == 0 && summary.running == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (items, summary) = manager.snapshot();
    panic!("queue did not drain: {summary:?}\n{items:#?}");
}

pub(crate) fn assert_all_done(manager: &Arc<TransferManager>) {
    let (items, _) = manager.snapshot();
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
