use super::sequencing::TransferCommandSequences;
use super::{
    AppliedTransferEvent, ApplyTransferEvent, TransferCommandError, TransferEffectBatch,
    TransferEffectDispatchError, TransferEffectDispatcher, TransferLoadError, TransferRepository,
    TransferSaveError, VersionedTransfer,
};

pub struct TransferCommandHandler<R, D> {
    repository: R,
    dispatcher: D,
    sequences: TransferCommandSequences,
}

impl<R, D> TransferCommandHandler<R, D>
where
    R: TransferRepository,
    D: TransferEffectDispatcher,
{
    pub fn new(repository: R, dispatcher: D) -> Self {
        Self {
            repository,
            dispatcher,
            sequences: TransferCommandSequences::default(),
        }
    }

    pub async fn handle(
        &self,
        command: ApplyTransferEvent,
    ) -> Result<AppliedTransferEvent, TransferCommandError> {
        let transfer_id = command.transfer_id();
        let sequence = self.sequences.gate(transfer_id);
        let _sequence_guard = sequence.lock().await;
        if let Some(pending) = self.sequences.pending_batch(transfer_id) {
            self.dispatcher.dispatch(&pending).await.map_err(
                |TransferEffectDispatchError::Unavailable| {
                    TransferCommandError::EffectDispatchFailed {
                        batch: pending.clone(),
                    }
                },
            )?;
            self.sequences.clear_pending(transfer_id);
        }
        let current = self
            .repository
            .load(transfer_id)
            .await
            .map_err(|TransferLoadError::Unavailable| {
                TransferCommandError::RepositoryUnavailable { transfer_id }
            })?
            .ok_or(TransferCommandError::NotFound { transfer_id })?;

        let expected_revision = current.revision();
        let transition = current
            .transfer()
            .transition(command.event())
            .map_err(|reason| TransferCommandError::InvalidTransition {
                transfer_id,
                reason,
            })?;

        if transition.effects.is_empty() && transition.next == *current.transfer() {
            return Ok(AppliedTransferEvent { versioned: current });
        }

        let committed_transfer = transition.next;
        let committed_revision = self
            .repository
            .save(expected_revision, committed_transfer.clone())
            .await
            .map_err(|error| match error {
                TransferSaveError::ConcurrentRevision { actual_revision } => {
                    TransferCommandError::ConcurrentRevision {
                        transfer_id,
                        expected_revision,
                        actual_revision,
                    }
                }
                TransferSaveError::Unavailable => {
                    TransferCommandError::RepositoryUnavailable { transfer_id }
                }
            })?;

        let batch = TransferEffectBatch::new(transfer_id, committed_revision, transition.effects);
        if let Err(TransferEffectDispatchError::Unavailable) =
            self.dispatcher.dispatch(&batch).await
        {
            self.sequences.remember_pending(batch.clone());
            return Err(TransferCommandError::EffectDispatchFailed { batch });
        }

        Ok(AppliedTransferEvent {
            versioned: VersionedTransfer::new(committed_transfer, committed_revision),
        })
    }
}
