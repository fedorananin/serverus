use super::{AttemptNumber, CompletionOutcome, ConflictKind, FailureKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransferState {
    Queued,
    Running {
        attempt: AttemptNumber,
    },
    Paused {
        attempt: AttemptNumber,
    },
    WaitingForConflict {
        attempt: AttemptNumber,
        conflict: ConflictKind,
    },
    WaitingForRetry {
        attempt: AttemptNumber,
        last_failure: FailureKind,
    },
    Cancelling {
        attempt: AttemptNumber,
    },
    Completed {
        outcome: CompletionOutcome,
    },
    Cancelled,
    Failed {
        failure: FailureKind,
    },
}

impl TransferState {
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Cancelled | Self::Failed { .. }
        )
    }

    pub const fn kind(&self) -> TransferStateKind {
        match self {
            Self::Queued => TransferStateKind::Queued,
            Self::Running { .. } => TransferStateKind::Running,
            Self::Paused { .. } => TransferStateKind::Paused,
            Self::WaitingForConflict { .. } => TransferStateKind::WaitingForConflict,
            Self::WaitingForRetry { .. } => TransferStateKind::WaitingForRetry,
            Self::Cancelling { .. } => TransferStateKind::Cancelling,
            Self::Completed { .. } => TransferStateKind::Completed,
            Self::Cancelled => TransferStateKind::Cancelled,
            Self::Failed { .. } => TransferStateKind::Failed,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferStateKind {
    Queued,
    Running,
    Paused,
    WaitingForConflict,
    WaitingForRetry,
    Cancelling,
    Completed,
    Cancelled,
    Failed,
}
