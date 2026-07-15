use tokio::sync::{oneshot, watch};

use serverus_domain::transfers::{
    AttemptNumber as DomainAttemptNumber, ConflictDecision as DomainConflictDecision,
    InvalidTransition as DomainInvalidTransition, RetryBudget as DomainRetryBudget,
    Transfer as DomainTransfer, TransferEffect as DomainTransferEffect,
    TransferEvent as DomainTransferEvent, TransferId as DomainTransferId,
    TransferState as DomainTransferState, TransferStateKind as DomainTransferStateKind,
};

use super::{ConflictAction, TransferState};

pub(super) const AUTO_RETRIES: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Control {
    Run,
    Pause,
    Cancel,
}

pub(super) struct PendingRetry {
    pub(super) claim: RetryClaim,
    pub(super) cancel: oneshot::Sender<()>,
}

pub(super) fn domain_state_to_ipc(state: &DomainTransferState) -> TransferState {
    match state {
        DomainTransferState::Queued => TransferState::Queued,
        DomainTransferState::Running { .. } => TransferState::Running,
        DomainTransferState::Paused { .. } => TransferState::Paused,
        DomainTransferState::WaitingForConflict { .. } => TransferState::Conflict,
        DomainTransferState::WaitingForRetry { .. } => TransferState::Error,
        DomainTransferState::Cancelling { .. } => TransferState::Running,
        DomainTransferState::Completed { outcome } => match outcome {
            serverus_domain::transfers::CompletionOutcome::Transferred => TransferState::Done,
            serverus_domain::transfers::CompletionOutcome::Skipped => TransferState::Skipped,
        },
        DomainTransferState::Cancelled => TransferState::Cancelled,
        DomainTransferState::Failed { .. } => TransferState::Error,
    }
}

pub(super) fn domain_conflict_decision(action: ConflictAction) -> DomainConflictDecision {
    match action {
        ConflictAction::Overwrite => DomainConflictDecision::Overwrite,
        ConflictAction::Skip => DomainConflictDecision::Skip,
        ConflictAction::Rename => DomainConflictDecision::Rename,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RetryClaim {
    pub(super) run_generation: u64,
    pub(super) attempt: DomainAttemptNumber,
}

pub(super) struct TransferLifecycle {
    pub(super) transfer: DomainTransfer,
    pub(super) error: Option<String>,
    run_generation: u64,
}

impl TransferLifecycle {
    pub(super) fn new(transfer_id: DomainTransferId, retry_budget: DomainRetryBudget) -> Self {
        Self {
            transfer: DomainTransfer::queued(transfer_id, retry_budget),
            error: None,
            run_generation: 0,
        }
    }

    pub(super) fn apply(
        &mut self,
        event: DomainTransferEvent,
        error: Option<String>,
    ) -> Result<Vec<DomainTransferEffect>, DomainInvalidTransition> {
        let transition = self.transfer.transition(event)?;
        let has_effects = !transition.effects.is_empty();
        self.transfer = transition.next;
        if has_effects {
            self.error = error;
        }
        Ok(transition.effects)
    }

    pub(super) fn restart_for_manual_retry(&mut self) {
        let transfer_id = self.transfer.id();
        let retry_budget = self.transfer.retry_budget();
        let run_generation = self.run_generation.wrapping_add(1);
        *self = Self {
            transfer: DomainTransfer::queued(transfer_id, retry_budget),
            error: None,
            run_generation,
        };
    }

    pub(super) fn retry_claim(&self, attempt: DomainAttemptNumber) -> Option<RetryClaim> {
        match self.transfer.state() {
            DomainTransferState::WaitingForRetry {
                attempt: current, ..
            } if *current == attempt => Some(RetryClaim {
                run_generation: self.run_generation,
                attempt,
            }),
            _ => None,
        }
    }

    pub(super) fn can_begin_manual_retry(&self) -> bool {
        matches!(
            self.transfer.state(),
            DomainTransferState::WaitingForRetry { .. }
                | DomainTransferState::Cancelled
                | DomainTransferState::Failed { .. }
        )
    }

    pub(super) fn ipc_state(&self) -> TransferState {
        domain_state_to_ipc(self.transfer.state())
    }

    pub(super) fn domain_state_kind(&self) -> DomainTransferStateKind {
        self.transfer.state().kind()
    }
}

pub(super) fn new_control_channel() -> watch::Sender<Control> {
    let (sender, _) = watch::channel(Control::Run);
    sender
}
