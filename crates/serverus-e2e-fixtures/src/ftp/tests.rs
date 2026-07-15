use std::io::Cursor;

use tokio::io::AsyncReadExt;

use super::{FailAfter, RetrievalFaults, FAULT_AFTER_BYTES};

#[test]
fn remote_edit_fault_rejects_every_failure_staging_promotion() {
    let directory = tempfile::tempdir().unwrap();
    let faults = RetrievalFaults::new(directory.path().join("ftp-retrievals.jsonl"));

    assert!(faults.reject_edit_promotion("/.serverus-edit-first.tmp", "/edit-failure.txt"));
    assert!(faults.reject_edit_promotion("/.serverus-edit-second.tmp", "/edit-failure.txt"));
    assert!(!faults.reject_edit_promotion("/.serverus-replace-original.bak", "/edit-failure.txt"));
    assert!(!faults.reject_edit_promotion("/.serverus-edit-success.tmp", "/edit-success.txt"));
}

#[test]
fn resume_fixture_fails_exactly_three_retrievals_and_records_only_offsets() {
    let directory = tempfile::tempdir().unwrap();
    let telemetry = directory.path().join("ftp-retrievals.jsonl");
    let faults = RetrievalFaults::new(telemetry.clone());

    assert_eq!(
        faults.fail_after("/resume.bin", 0).unwrap(),
        Some(FAULT_AFTER_BYTES)
    );
    assert_eq!(
        faults.fail_after("resume.bin", 65_536).unwrap(),
        Some(FAULT_AFTER_BYTES)
    );
    assert_eq!(
        faults.fail_after("/resume.bin", 131_072).unwrap(),
        Some(FAULT_AFTER_BYTES)
    );
    assert_eq!(faults.fail_after("/resume.bin", 196_608).unwrap(), None);
    assert_eq!(faults.fail_after("/ordinary.bin", 0).unwrap(), None);

    let lines = std::fs::read_to_string(telemetry).unwrap();
    assert_eq!(
        lines.lines().collect::<Vec<_>>(),
        [
            r#"{"start_pos":0}"#,
            r#"{"start_pos":65536}"#,
            r#"{"start_pos":131072}"#,
            r#"{"start_pos":196608}"#,
        ]
    );
}

#[tokio::test]
async fn failing_reader_delivers_the_prefix_before_interrupting_the_transfer() {
    let source = Cursor::new(vec![7_u8; 16]);
    let mut reader = FailAfter::new(source, 5);
    let mut bytes = Vec::new();

    let error = reader.read_to_end(&mut bytes).await.unwrap_err();

    assert_eq!(bytes, vec![7_u8; 5]);
    assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset);
}
