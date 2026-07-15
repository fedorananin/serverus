use std::sync::atomic::Ordering;

use serverus_domain::transfers::{
    AttemptNumber as DomainAttemptNumber, FailureKind as DomainFailureKind,
    InvalidTransition as DomainInvalidTransition, TransferEffect as DomainTransferEffect,
    TransferEvent as DomainTransferEvent, TransferStateKind as DomainTransferStateKind,
};
use tokio::sync::oneshot;

use crate::error::AppResult;

use super::lifecycle::{domain_state_to_ipc, PendingRetry};
use super::{
    ConflictAction, Control, RetryClaim, TransferItem, TransferLifecycle, TransferSnapshot,
    TransferState,
};

enum PartialDisposition {
    Clear,
    Cleanup,
    Keep,
}

struct WorkerFinalization {
    effects: Vec<DomainTransferEffect>,
    partial: PartialDisposition,
}

impl TransferItem {
    pub(super) fn snapshot(&self) -> TransferSnapshot {
        let lifecycle = self.lifecycle.lock().unwrap();
        TransferSnapshot {
            id: self.id.clone(),
            session_id: self.session_id.clone(),
            kind: self.kind,
            state: lifecycle.ipc_state(),
            error: lifecycle.error.clone(),
            name: self.name.clone(),
            local_path: self.local_path.to_string_lossy().into_owned(),
            remote_path: self.remote_path.clone(),
            accelerated: self.tar.is_some(),
            done: self.done.load(Ordering::Relaxed),
            total: self.total.load(Ordering::Relaxed),
            speed_bps: self.speed_bps.load(Ordering::Relaxed),
        }
    }

    pub(super) fn state(&self) -> TransferState {
        let lifecycle = self.lifecycle.lock().unwrap();
        domain_state_to_ipc(lifecycle.transfer.state())
    }

    pub(super) fn apply_and_dispatch(
        &self,
        event: DomainTransferEvent,
        error: Option<String>,
        conflict_action: Option<ConflictAction>,
    ) -> Result<Vec<DomainTransferEffect>, DomainInvalidTransition> {
        let mut lifecycle = self.lifecycle.lock().unwrap();
        self.apply_and_dispatch_locked(&mut lifecycle, event, error, conflict_action)
    }

    fn apply_and_dispatch_locked(
        &self,
        lifecycle: &mut TransferLifecycle,
        event: DomainTransferEvent,
        error: Option<String>,
        conflict_action: Option<ConflictAction>,
    ) -> Result<Vec<DomainTransferEffect>, DomainInvalidTransition> {
        let effects = lifecycle.apply(event, error)?;
        dispatch_immediate_effects(self, &effects, conflict_action);
        Ok(effects)
    }

    fn finalize_worker(&self, result: AppResult<TransferState>) -> WorkerFinalization {
        let mut lifecycle = self.lifecycle.lock().unwrap();
        let mut all_effects = Vec::new();

        if lifecycle.domain_state_kind() == DomainTransferStateKind::Paused {
            if let Ok(effects) = self.apply_and_dispatch_locked(
                &mut lifecycle,
                DomainTransferEvent::ResumeRequested,
                None,
                None,
            ) {
                all_effects.extend(effects);
            }
        }

        if lifecycle.domain_state_kind() == DomainTransferStateKind::Cancelling {
            if let Ok(effects) = self.apply_and_dispatch_locked(
                &mut lifecycle,
                DomainTransferEvent::CancellationFinished,
                None,
                None,
            ) {
                all_effects.extend(effects);
            }
            return WorkerFinalization {
                effects: all_effects,
                partial: PartialDisposition::Cleanup,
            };
        }

        let (events, error) = match result {
            Ok(TransferState::Done) => (vec![DomainTransferEvent::AttemptSucceeded], None),
            Ok(TransferState::Skipped) => (Vec::new(), None),
            Ok(TransferState::Cancelled) => (
                vec![
                    DomainTransferEvent::CancelRequested,
                    DomainTransferEvent::CancellationFinished,
                ],
                None,
            ),
            Ok(_) => (Vec::new(), None),
            Err(error) => {
                let event = if self.tar.is_some() {
                    DomainTransferEvent::PermanentFailure(DomainFailureKind::RemoteIo)
                } else {
                    DomainTransferEvent::RecoverableFailure(DomainFailureKind::RemoteIo)
                };
                (vec![event], Some(error.to_string()))
            }
        };

        for event in events {
            let Ok(effects) =
                self.apply_and_dispatch_locked(&mut lifecycle, event, error.clone(), None)
            else {
                break;
            };
            all_effects.extend(effects);
        }
        let partial = match lifecycle.domain_state_kind() {
            DomainTransferStateKind::Completed => PartialDisposition::Clear,
            DomainTransferStateKind::Cancelled => PartialDisposition::Cleanup,
            _ => PartialDisposition::Keep,
        };
        WorkerFinalization {
            effects: all_effects,
            partial,
        }
    }

    pub(super) async fn complete_worker(
        &self,
        result: AppResult<TransferState>,
    ) -> Vec<DomainTransferEffect> {
        let finalization = self.finalize_worker(result);
        match finalization.partial {
            PartialDisposition::Clear => self.clear_partial(),
            PartialDisposition::Cleanup => self.cleanup_partial().await,
            PartialDisposition::Keep => {}
        }
        finalization.effects
    }

    pub(super) fn retry_claim(&self, attempt: DomainAttemptNumber) -> Option<RetryClaim> {
        self.lifecycle.lock().unwrap().retry_claim(attempt)
    }

    pub(super) fn install_pending_retry(
        &self,
        claim: RetryClaim,
        cancel: oneshot::Sender<()>,
    ) -> bool {
        let lifecycle = self.lifecycle.lock().unwrap();
        if lifecycle.retry_claim(claim.attempt) != Some(claim) {
            return false;
        }
        let mut pending = self.pending_retry.lock().unwrap();
        if pending.is_some() {
            return false;
        }
        *pending = Some(PendingRetry { claim, cancel });
        true
    }

    pub(super) fn begin_manual_retry(&self) -> bool {
        let mut lifecycle = self.lifecycle.lock().unwrap();
        if !lifecycle.can_begin_manual_retry() {
            return false;
        }
        if let Some(pending) = self.pending_retry.lock().unwrap().take() {
            let _ = pending.cancel.send(());
        }
        lifecycle.restart_for_manual_retry();
        true
    }

    pub(super) fn claim_auto_retry(&self, claim: RetryClaim) -> bool {
        let mut lifecycle = self.lifecycle.lock().unwrap();
        if lifecycle.retry_claim(claim.attempt) != Some(claim) {
            return false;
        }
        let mut pending = self.pending_retry.lock().unwrap();
        if !pending
            .as_ref()
            .is_some_and(|pending| pending.claim == claim)
        {
            return false;
        }
        pending.take();
        let Ok(effects) = lifecycle.apply(DomainTransferEvent::RetryDelayElapsed, None) else {
            return false;
        };
        dispatch_immediate_effects(self, &effects, None);
        true
    }

    pub(super) fn cancel_pending_retry(&self) {
        if let Some(pending) = self.pending_retry.lock().unwrap().take() {
            let _ = pending.cancel.send(());
        }
    }
}

fn dispatch_immediate_effects(
    item: &TransferItem,
    effects: &[DomainTransferEffect],
    conflict_action: Option<ConflictAction>,
) {
    for effect in effects {
        match effect {
            DomainTransferEffect::StartAttempt { .. }
            | DomainTransferEffect::ResumeAttempt { .. } => {
                item.control.send_replace(Control::Run);
            }
            DomainTransferEffect::PauseAttempt { .. } => {
                item.control.send_replace(Control::Pause);
            }
            DomainTransferEffect::ApplyConflictDecision { .. } => {
                if let Some(action) = conflict_action {
                    if let Some(sender) = item.resolver.lock().unwrap().take() {
                        let _ = sender.send(action);
                    }
                }
            }
            DomainTransferEffect::CancelAttempt { .. } => {
                item.control.send_replace(Control::Cancel);
                if let Some(sender) = item.resolver.lock().unwrap().take() {
                    let _ = sender.send(ConflictAction::Skip);
                }
            }
            DomainTransferEffect::CancelRetry { .. } => item.cancel_pending_retry(),
            DomainTransferEffect::RequestConflictDecision { .. }
            | DomainTransferEffect::ScheduleRetry { .. }
            | DomainTransferEffect::PublishTerminal { .. } => {}
        }
    }
}
