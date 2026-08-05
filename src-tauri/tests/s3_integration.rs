//! S3 integration tests (SPEC §4.4, §7.3) against a real in-process S3
//! server (`s3s` + filesystem backend).

#[path = "support/transfer_context.rs"]
mod transfer_context;

#[path = "s3_integration/acl_backend.rs"]
mod acl_backend;
#[path = "s3_integration/common.rs"]
mod common;
#[path = "s3_integration/compare.rs"]
mod compare;
#[path = "s3_integration/content_type.rs"]
mod content_type;
#[path = "s3_integration/multipart.rs"]
mod multipart;
#[path = "s3_integration/operations.rs"]
mod operations;
#[path = "s3_integration/server.rs"]
mod server;
#[path = "s3_integration/transfers.rs"]
mod transfers;

#[tokio::test]
async fn s3_bucket_level_operations() {
    operations::bucket_level_operations().await;
}

#[tokio::test]
async fn s3_recursive_directory_transfers() {
    transfers::recursive_directory_transfers().await;
}

#[tokio::test]
async fn s3_multipart_upload_roundtrip() {
    transfers::multipart_upload_roundtrip().await;
}

#[tokio::test]
async fn dropping_writer_aborts_multipart_after_create_while_first_part_is_pending() {
    multipart::dropping_writer_aborts_after_create_while_first_part_is_pending().await;
}

#[tokio::test]
async fn completed_multipart_upload_is_not_aborted() {
    multipart::completed_upload_is_not_aborted().await;
}

#[tokio::test]
async fn small_writer_still_uses_put_object_without_multipart() {
    multipart::small_writer_uses_put_object_without_multipart().await;
}

#[tokio::test]
async fn s3_uploads_declare_content_type_from_the_object_name() {
    content_type::put_object_uploads_declare_the_type().await;
}

#[tokio::test]
async fn s3_uploads_of_unknown_extensions_leave_the_sdk_default() {
    content_type::unknown_extensions_leave_the_sdk_default().await;
}

#[tokio::test]
async fn s3_multipart_uploads_declare_content_type() {
    content_type::multipart_uploads_declare_the_type().await;
}

#[tokio::test]
async fn s3_uploads_without_a_usable_name_are_identified_by_content() {
    content_type::nameless_files_are_identified_by_their_bytes().await;
}

#[tokio::test]
async fn s3_upload_content_type_prefers_the_name_over_the_bytes() {
    content_type::the_name_outranks_the_bytes().await;
}

#[tokio::test]
async fn s3_multipart_uploads_sniff_their_head() {
    content_type::multipart_uploads_sniff_their_head().await;
}

#[tokio::test]
async fn s3_created_empty_files_declare_content_type() {
    content_type::created_empty_files_declare_the_type().await;
}

#[tokio::test]
async fn s3_remote_edit_publishes_the_target_content_type() {
    content_type::remote_edit_publishes_the_targets_type_not_the_staging_name().await;
}

#[tokio::test]
async fn s3_acl_public_private_flow() {
    operations::acl_public_private_flow().await;
}

#[tokio::test]
async fn s3_replacement_staging_is_private_under_public_upload_mode() {
    operations::replacement_staging_is_private_under_public_upload_mode().await;
}

#[tokio::test]
async fn s3_subtree_compare() {
    compare::subtree_compare().await;
}
