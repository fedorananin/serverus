use std::future::Future;
use std::pin::Pin;
pub(super) use std::sync::Arc;
use std::sync::Mutex;
pub(super) use std::task::Poll;
use std::task::{Context, Waker};

use async_trait::async_trait;
pub(super) use serverus_application::transfers::{
    ApplyTransferEvent, TransferCommandError, TransferCommandHandler, TransferEffectBatch,
    TransferEffectDispatchError, TransferRevision, TransferSaveError, VersionedTransfer,
};
use serverus_application::transfers::{
    TransferEffectDispatcher, TransferLoadError, TransferRepository,
};
pub(super) use serverus_domain::transfers::{
    AttemptNumber, CompletionOutcome, ConflictDecision, RetryBudget, TerminalOutcome,
    TransferEffect, TransferEvent, TransferState, TransferStateKind,
};
use serverus_domain::transfers::{ConflictKind, Transfer, TransferId};

pub(super) fn transfer_id() -> TransferId {
    TransferId::try_from(42_u128).expect("test transfer ID is non-zero")
}

pub(super) fn queued_transfer() -> Transfer {
    Transfer::queued(transfer_id(), RetryBudget::new(2))
}

pub(super) fn running_transfer() -> Transfer {
    queued_transfer()
        .transition(TransferEvent::StartRequested)
        .expect("queued transfer starts")
        .next
}

pub(super) fn waiting_for_conflict() -> Transfer {
    queued_transfer()
        .transition(TransferEvent::StartRequested)
        .expect("queued transfer starts")
        .next
        .transition(TransferEvent::ConflictDetected(
            ConflictKind::DestinationExists,
        ))
        .expect("running transfer detects a conflict")
        .next
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum RecordedOperation {
    Persist {
        revision: TransferRevision,
        state: TransferStateKind,
    },
    Dispatch(TransferEffect),
}

#[derive(Clone)]
pub(super) struct FakeTransferRepository {
    state: Arc<Mutex<FakeRepositoryState>>,
    pub(super) operations: Arc<Mutex<Vec<RecordedOperation>>>,
}

struct FakeRepositoryState {
    stored: Option<VersionedTransfer>,
    load_error: Option<TransferLoadError>,
    save_error: Option<TransferSaveError>,
}

impl FakeTransferRepository {
    pub(super) fn with_transfer(transfer: Transfer, revision: TransferRevision) -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeRepositoryState {
                stored: Some(VersionedTransfer::new(transfer, revision)),
                load_error: None,
                save_error: None,
            })),
            operations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(super) fn empty() -> Self {
        Self {
            state: Arc::new(Mutex::new(FakeRepositoryState {
                stored: None,
                load_error: None,
                save_error: None,
            })),
            operations: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub(super) fn fail_load(&self) {
        self.state.lock().expect("repository lock").load_error =
            Some(TransferLoadError::Unavailable);
    }

    pub(super) fn fail_save_with(&self, error: TransferSaveError) {
        self.state.lock().expect("repository lock").save_error = Some(error);
    }

    pub(super) fn stored(&self) -> Option<VersionedTransfer> {
        self.state.lock().expect("repository lock").stored.clone()
    }
}

#[async_trait]
impl TransferRepository for FakeTransferRepository {
    async fn load(
        &self,
        transfer_id: TransferId,
    ) -> Result<Option<VersionedTransfer>, TransferLoadError> {
        let state = self.state.lock().expect("repository lock");
        if let Some(error) = state.load_error {
            return Err(error);
        }
        Ok(state
            .stored
            .as_ref()
            .filter(|stored| stored.transfer().id() == transfer_id)
            .cloned())
    }

    async fn save(
        &self,
        expected_revision: TransferRevision,
        transfer: Transfer,
    ) -> Result<TransferRevision, TransferSaveError> {
        let mut state = self.state.lock().expect("repository lock");
        if let Some(error) = state.save_error {
            return Err(error);
        }
        let actual_revision = state.stored.as_ref().map(VersionedTransfer::revision);
        if actual_revision != Some(expected_revision) {
            return Err(TransferSaveError::ConcurrentRevision { actual_revision });
        }
        let next_revision = expected_revision
            .checked_next()
            .expect("test revisions do not overflow");
        self.operations
            .lock()
            .expect("operation lock")
            .push(RecordedOperation::Persist {
                revision: next_revision,
                state: transfer.state().kind(),
            });
        state.stored = Some(VersionedTransfer::new(transfer, next_revision));
        Ok(next_revision)
    }
}

#[derive(Clone)]
pub(super) struct RecordingEffectDispatcher {
    batches: Arc<Mutex<Vec<TransferEffectBatch>>>,
    operations: Arc<Mutex<Vec<RecordedOperation>>>,
    failure: Arc<Mutex<Option<TransferEffectDispatchError>>>,
}

impl RecordingEffectDispatcher {
    pub(super) fn new(operations: Arc<Mutex<Vec<RecordedOperation>>>) -> Self {
        Self {
            batches: Arc::new(Mutex::new(Vec::new())),
            operations,
            failure: Arc::new(Mutex::new(None)),
        }
    }

    pub(super) fn fail(&self) {
        *self.failure.lock().expect("dispatcher lock") =
            Some(TransferEffectDispatchError::Unavailable);
    }

    pub(super) fn batches(&self) -> Vec<TransferEffectBatch> {
        self.batches.lock().expect("batch lock").clone()
    }
}

#[async_trait]
impl TransferEffectDispatcher for RecordingEffectDispatcher {
    async fn dispatch(
        &self,
        batch: &TransferEffectBatch,
    ) -> Result<(), TransferEffectDispatchError> {
        if let Some(error) = *self.failure.lock().expect("dispatcher lock") {
            return Err(error);
        }
        for effect in batch.effects() {
            self.operations
                .lock()
                .expect("operation lock")
                .push(RecordedOperation::Dispatch(*effect));
        }
        self.batches.lock().expect("batch lock").push(batch.clone());
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct GatedEffectDispatcher {
    blocked_revision: TransferRevision,
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
    operations: Arc<Mutex<Vec<RecordedOperation>>>,
}

impl GatedEffectDispatcher {
    pub(super) fn new(
        blocked_revision: TransferRevision,
        operations: Arc<Mutex<Vec<RecordedOperation>>>,
    ) -> Self {
        Self {
            blocked_revision,
            entered: Arc::new(tokio::sync::Notify::new()),
            release: Arc::new(tokio::sync::Notify::new()),
            operations,
        }
    }

    pub(super) async fn wait_until_blocked(&self) {
        self.entered.notified().await;
    }

    pub(super) fn release(&self) {
        self.release.notify_one();
    }
}

#[async_trait]
impl TransferEffectDispatcher for GatedEffectDispatcher {
    async fn dispatch(
        &self,
        batch: &TransferEffectBatch,
    ) -> Result<(), TransferEffectDispatchError> {
        if batch.revision() == self.blocked_revision {
            self.entered.notify_one();
            self.release.notified().await;
        }
        for effect in batch.effects() {
            self.operations
                .lock()
                .expect("operation lock")
                .push(RecordedOperation::Dispatch(*effect));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub(super) struct FailingThenRecordingDispatcher {
    failures_remaining: Arc<Mutex<usize>>,
    operations: Arc<Mutex<Vec<RecordedOperation>>>,
}

impl FailingThenRecordingDispatcher {
    pub(super) fn new(failures: usize, operations: Arc<Mutex<Vec<RecordedOperation>>>) -> Self {
        Self {
            failures_remaining: Arc::new(Mutex::new(failures)),
            operations,
        }
    }
}

#[async_trait]
impl TransferEffectDispatcher for FailingThenRecordingDispatcher {
    async fn dispatch(
        &self,
        batch: &TransferEffectBatch,
    ) -> Result<(), TransferEffectDispatchError> {
        let should_fail = {
            let mut failures = self.failures_remaining.lock().expect("failure lock");
            if *failures == 0 {
                false
            } else {
                *failures -= 1;
                true
            }
        };
        if should_fail {
            return Err(TransferEffectDispatchError::Unavailable);
        }
        for effect in batch.effects() {
            self.operations
                .lock()
                .expect("operation lock")
                .push(RecordedOperation::Dispatch(*effect));
        }
        Ok(())
    }
}

pub(super) fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}
