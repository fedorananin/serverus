//! Integration tests for SFTP file operations and the transfer queue (M3)
//! against a real unprivileged sshd + sftp-server.

mod support;
#[path = "support/transfer_context.rs"]
mod transfer_context;

#[path = "sftp_integration/common.rs"]
mod common;
#[path = "sftp_integration/conflicts.rs"]
mod conflicts;
#[path = "sftp_integration/operations.rs"]
mod operations;
#[path = "sftp_integration/transfers.rs"]
mod transfers;

#[tokio::test]
async fn sftp_basic_operations() {
    operations::basic_operations().await;
}

#[tokio::test]
async fn recursive_roundtrip_through_queue() {
    transfers::recursive_roundtrip_through_queue().await;
}

#[tokio::test]
async fn conflict_policies() {
    conflicts::conflict_policies().await;
}
