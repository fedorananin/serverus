use std::sync::Arc;
use std::time::Duration;

use serverus_lib::session::ssh::{connect_chain, ConnectOutcome, SshSession};
use serverus_lib::transfer::{ProgressSink, TransferManager, TransferState};
use serverus_lib::vault::model::{ConflictPolicy, TransferSettings};

use crate::support::TestSshd;

pub(crate) struct NullSink;

impl ProgressSink for NullSink {
    fn emit(&self, _event: serverus_lib::events::TransferProgressEvent) {}
}

pub(crate) async fn connect(sshd: &TestSshd) -> SshSession {
    let issue = match connect_chain(&[sshd.hop(None)]).await.unwrap() {
        ConnectOutcome::HostKeyPrompt(issue) => issue,
        _ => panic!("expected host key prompt"),
    };
    match connect_chain(&[sshd.hop(Some(issue.key_line))])
        .await
        .unwrap()
    {
        ConnectOutcome::Connected(handle) => SshSession {
            handle: tokio::sync::Mutex::new(handle),
        },
        _ => panic!("expected connection"),
    }
}

pub(crate) fn settings() -> TransferSettings {
    TransferSettings {
        max_parallel_per_server: 4,
        conflict_policy: ConflictPolicy::Overwrite,
        preserve_mtime: true,
        tar_acceleration: false,
    }
}

pub(crate) async fn wait_for_drain(manager: &Arc<TransferManager>) {
    for _ in 0..300 {
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
