use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use futures::lock::Mutex as AsyncMutex;
use serverus_domain::transfers::TransferId;

use super::TransferEffectBatch;

#[derive(Default)]
pub(super) struct TransferCommandSequences {
    gates: Mutex<HashMap<TransferId, Weak<AsyncMutex<()>>>>,
    pending_batches: Mutex<HashMap<TransferId, TransferEffectBatch>>,
}

impl TransferCommandSequences {
    pub(super) fn gate(&self, transfer_id: TransferId) -> Arc<AsyncMutex<()>> {
        let mut gates = self.gates.lock().expect("transfer sequence registry lock");
        gates.retain(|_, gate| gate.strong_count() > 0);
        if let Some(gate) = gates.get(&transfer_id).and_then(Weak::upgrade) {
            return gate;
        }

        let gate = Arc::new(AsyncMutex::new(()));
        gates.insert(transfer_id, Arc::downgrade(&gate));
        gate
    }

    pub(super) fn pending_batch(&self, transfer_id: TransferId) -> Option<TransferEffectBatch> {
        self.pending_batches
            .lock()
            .expect("pending transfer batch lock")
            .get(&transfer_id)
            .cloned()
    }

    pub(super) fn remember_pending(&self, batch: TransferEffectBatch) {
        self.pending_batches
            .lock()
            .expect("pending transfer batch lock")
            .insert(batch.transfer_id(), batch);
    }

    pub(super) fn clear_pending(&self, transfer_id: TransferId) {
        self.pending_batches
            .lock()
            .expect("pending transfer batch lock")
            .remove(&transfer_id);
    }
}
