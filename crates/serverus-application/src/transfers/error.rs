use serverus_domain::transfers::{InvalidTransition, TransferId};
use thiserror::Error;

use super::{TransferEffectBatch, TransferRevision};

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum TransferCommandError {
    #[error("transfer {transfer_id:?} was not found")]
    NotFound { transfer_id: TransferId },
    #[error("transfer {transfer_id:?} rejected the requested transition")]
    InvalidTransition {
        transfer_id: TransferId,
        #[source]
        reason: InvalidTransition,
    },
    #[error("transfer {transfer_id:?} changed concurrently")]
    ConcurrentRevision {
        transfer_id: TransferId,
        expected_revision: TransferRevision,
        actual_revision: Option<TransferRevision>,
    },
    #[error("repository is unavailable while handling transfer {transfer_id:?}")]
    RepositoryUnavailable { transfer_id: TransferId },
    #[error("effects for a committed transfer transition could not be dispatched")]
    EffectDispatchFailed { batch: TransferEffectBatch },
}
