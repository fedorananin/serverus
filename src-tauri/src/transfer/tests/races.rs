use std::sync::{Arc, Barrier};

use serverus_domain::transfers::{
    TransferEvent as DomainTransferEvent, TransferStateKind as DomainTransferStateKind,
};

use super::super::{open_download_root, LocalDownloadTarget, TransferState};
use super::support::{item, start};

#[tokio::test]
async fn worker_completion_after_cancel_reaches_cancelled_atomically() {
    let transfer = item("session");
    start(&transfer);
    transfer
        .apply_and_dispatch(DomainTransferEvent::CancelRequested, None, None)
        .unwrap();

    transfer.complete_worker(Ok(TransferState::Done)).await;

    assert_eq!(transfer.state(), TransferState::Cancelled);
    assert_eq!(
        transfer.domain_state_kind(),
        DomainTransferStateKind::Cancelled
    );
}

#[tokio::test]
async fn worker_completion_after_pause_reaches_done_atomically() {
    let transfer = item("session");
    start(&transfer);
    transfer
        .apply_and_dispatch(DomainTransferEvent::PauseRequested, None, None)
        .unwrap();

    transfer.complete_worker(Ok(TransferState::Done)).await;

    assert_eq!(transfer.state(), TransferState::Done);
    assert_eq!(
        transfer.domain_state_kind(),
        DomainTransferStateKind::Completed
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancel_racing_worker_completion_never_leaves_a_finished_worker_cancelling() {
    for _ in 0..64 {
        let transfer = item("session");
        start(&transfer);
        let barrier = Arc::new(Barrier::new(3));

        let control_item = transfer.clone();
        let control_barrier = barrier.clone();
        let cancel = std::thread::spawn(move || {
            control_barrier.wait();
            let _ =
                control_item.apply_and_dispatch(DomainTransferEvent::CancelRequested, None, None);
        });
        let worker_item = transfer.clone();
        let worker_barrier = barrier.clone();
        let runtime = tokio::runtime::Handle::current();
        let complete = std::thread::spawn(move || {
            worker_barrier.wait();
            runtime.block_on(worker_item.complete_worker(Ok(TransferState::Done)));
        });
        barrier.wait();
        cancel.join().unwrap();
        complete.join().unwrap();

        assert!(matches!(
            transfer.domain_state_kind(),
            DomainTransferStateKind::Completed | DomainTransferStateKind::Cancelled
        ));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pause_racing_worker_completion_never_leaves_a_finished_worker_paused() {
    for _ in 0..64 {
        let transfer = item("session");
        start(&transfer);
        let barrier = Arc::new(Barrier::new(3));

        let control_item = transfer.clone();
        let control_barrier = barrier.clone();
        let pause = std::thread::spawn(move || {
            control_barrier.wait();
            let _ =
                control_item.apply_and_dispatch(DomainTransferEvent::PauseRequested, None, None);
        });
        let worker_item = transfer.clone();
        let worker_barrier = barrier.clone();
        let runtime = tokio::runtime::Handle::current();
        let complete = std::thread::spawn(move || {
            worker_barrier.wait();
            runtime.block_on(worker_item.complete_worker(Ok(TransferState::Done)));
        });
        barrier.wait();
        pause.join().unwrap();
        complete.join().unwrap();

        assert_eq!(
            transfer.domain_state_kind(),
            DomainTransferStateKind::Completed
        );
    }
}

#[tokio::test]
async fn cancel_after_copy_before_finalize_removes_the_completed_target() {
    let transfer = item("session");
    let directory = tempfile::tempdir().unwrap();
    let completed_target = directory.path().join("completed-download");
    std::fs::write(&completed_target, b"complete payload").unwrap();
    transfer.mark_local_partial(LocalDownloadTarget {
        root: open_download_root(directory.path()).unwrap(),
        relative: "completed-download".into(),
    });
    start(&transfer);

    transfer
        .apply_and_dispatch(DomainTransferEvent::CancelRequested, None, None)
        .unwrap();
    transfer.complete_worker(Ok(TransferState::Done)).await;

    assert_eq!(transfer.state(), TransferState::Cancelled);
    assert!(
        !completed_target.exists(),
        "cancelled transfer left a completed target behind"
    );
}
