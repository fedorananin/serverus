use serverus_domain::transfers::{Transfer, TransferEffect, TransferEvent, TransferId};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TransferRevision(u64);

impl TransferRevision {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn checked_next(self) -> Option<Self> {
        match self.0.checked_add(1) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionedTransfer {
    transfer: Transfer,
    revision: TransferRevision,
}

impl VersionedTransfer {
    pub const fn new(transfer: Transfer, revision: TransferRevision) -> Self {
        Self { transfer, revision }
    }

    pub const fn transfer(&self) -> &Transfer {
        &self.transfer
    }

    pub const fn revision(&self) -> TransferRevision {
        self.revision
    }

    pub fn into_parts(self) -> (Transfer, TransferRevision) {
        (self.transfer, self.revision)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ApplyTransferEvent {
    transfer_id: TransferId,
    event: TransferEvent,
}

impl ApplyTransferEvent {
    pub const fn new(transfer_id: TransferId, event: TransferEvent) -> Self {
        Self { transfer_id, event }
    }

    pub const fn transfer_id(self) -> TransferId {
        self.transfer_id
    }

    pub const fn event(self) -> TransferEvent {
        self.event
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferEffectBatch {
    transfer_id: TransferId,
    revision: TransferRevision,
    effects: Vec<TransferEffect>,
}

impl TransferEffectBatch {
    pub(super) fn new(
        transfer_id: TransferId,
        revision: TransferRevision,
        effects: Vec<TransferEffect>,
    ) -> Self {
        Self {
            transfer_id,
            revision,
            effects,
        }
    }

    pub const fn transfer_id(&self) -> TransferId {
        self.transfer_id
    }

    pub const fn revision(&self) -> TransferRevision {
        self.revision
    }

    pub fn effects(&self) -> &[TransferEffect] {
        &self.effects
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppliedTransferEvent {
    pub(super) versioned: VersionedTransfer,
}

impl AppliedTransferEvent {
    pub const fn transfer(&self) -> &Transfer {
        self.versioned.transfer()
    }

    pub const fn revision(&self) -> TransferRevision {
        self.versioned.revision()
    }

    pub fn into_versioned(self) -> VersionedTransfer {
        self.versioned
    }
}
