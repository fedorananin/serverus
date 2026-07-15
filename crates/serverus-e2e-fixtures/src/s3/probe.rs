use std::sync::atomic::{AtomicBool, AtomicUsize};

/// Optional counters and synchronization points used by multipart integration
/// tests. The production E2E fixture passes `None` and pays no runtime cost.
#[derive(Default)]
pub struct MultipartProbe {
    pub block_upload_part: AtomicBool,
    pub create_calls: AtomicUsize,
    pub upload_part_calls: AtomicUsize,
    pub complete_calls: AtomicUsize,
    pub abort_calls: AtomicUsize,
    pub put_object_calls: AtomicUsize,
    pub upload_part_started: tokio::sync::Notify,
    pub release_upload_part: tokio::sync::Notify,
    pub abort_seen: tokio::sync::Notify,
}
