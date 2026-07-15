use async_trait::async_trait;
use serverus_domain::transfers::{Transfer, TransferId};
use thiserror::Error;

use super::{TransferEffectBatch, TransferRevision, VersionedTransfer};

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TransferLoadError {
    #[error("transfer repository is unavailable")]
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TransferSaveError {
    #[error("transfer revision changed concurrently")]
    ConcurrentRevision {
        actual_revision: Option<TransferRevision>,
    },
    #[error("transfer repository is unavailable")]
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum TransferEffectDispatchError {
    #[error("transfer effect dispatcher is unavailable")]
    Unavailable,
}

#[async_trait]
pub trait TransferRepository: Send + Sync {
    async fn load(
        &self,
        transfer_id: TransferId,
    ) -> Result<Option<VersionedTransfer>, TransferLoadError>;

    async fn save(
        &self,
        expected_revision: TransferRevision,
        transfer: Transfer,
    ) -> Result<TransferRevision, TransferSaveError>;
}

#[async_trait]
pub trait TransferEffectDispatcher: Send + Sync {
    async fn dispatch(
        &self,
        batch: &TransferEffectBatch,
    ) -> Result<(), TransferEffectDispatchError>;
}
