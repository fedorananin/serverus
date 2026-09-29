//! Read-only lookups into the queue by item id, for callers that enqueue
//! work and then follow exactly their own items (the AI agent).

use std::sync::Arc;

use super::{TransferBatch, TransferManager, TransferSnapshot, TransferState};
use crate::error::AppResult;

/// What one enqueue call did: the batch its items share (items of a
/// recursive transfer may join it later) and whether every path queued.
pub struct Enqueued {
    pub batch: String,
    pub result: AppResult<()>,
}

impl Enqueued {
    pub(super) fn new(batch: &Arc<TransferBatch>, result: AppResult<()>) -> Self {
        Self {
            batch: batch.id().to_string(),
            result,
        }
    }
}

impl TransferManager {
    /// Ids of every item currently queued for (or finished in) a session,
    /// in queue order.
    pub fn session_item_ids(&self, session_id: &str) -> Vec<String> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .filter(|item| item.session_id == session_id)
            .map(|item| item.id.clone())
            .collect()
    }

    /// Ids of the items of one batch, in queue order.
    pub fn batch_item_ids(&self, batch: &str) -> Vec<String> {
        self.items
            .lock()
            .unwrap()
            .iter()
            .filter(|item| item.batch.id().to_string() == batch)
            .map(|item| item.id.clone())
            .collect()
    }

    /// Current snapshots of the given items, in the given order. Items that
    /// are gone (cleared, session closed) are skipped.
    pub fn item_snapshots(&self, ids: &[String]) -> Vec<TransferSnapshot> {
        let items = self.items.lock().unwrap();
        ids.iter()
            .filter_map(|id| items.iter().find(|item| &item.id == id))
            .map(|item| item.snapshot())
            .collect()
    }
}

/// Whether an item has reached a state it will not leave on its own.
pub fn is_settled(state: &TransferState) -> bool {
    matches!(
        state,
        TransferState::Done
            | TransferState::Skipped
            | TransferState::Cancelled
            | TransferState::Error
    )
}
