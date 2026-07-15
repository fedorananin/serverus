use super::AttemptNumber;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictKind {
    DestinationExists,
    ResourceTypeMismatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictDecision {
    Overwrite,
    Rename,
    Skip,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompletionOutcome {
    Transferred,
    Skipped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    NetworkInterrupted,
    EndpointBusy,
    LocalIo,
    RemoteIo,
    Integrity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalOutcome {
    Completed(CompletionOutcome),
    Cancelled,
    Failed(FailureKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferEvent {
    StartRequested,
    PauseRequested,
    ResumeRequested,
    ConflictDetected(ConflictKind),
    ConflictResolved(ConflictDecision),
    RecoverableFailure(FailureKind),
    RetryDelayElapsed,
    AttemptSucceeded,
    PermanentFailure(FailureKind),
    CancelRequested,
    CancellationFinished,
}

impl TransferEvent {
    pub const fn kind(self) -> TransferEventKind {
        match self {
            Self::StartRequested => TransferEventKind::StartRequested,
            Self::PauseRequested => TransferEventKind::PauseRequested,
            Self::ResumeRequested => TransferEventKind::ResumeRequested,
            Self::ConflictDetected(_) => TransferEventKind::ConflictDetected,
            Self::ConflictResolved(_) => TransferEventKind::ConflictResolved,
            Self::RecoverableFailure(_) => TransferEventKind::RecoverableFailure,
            Self::RetryDelayElapsed => TransferEventKind::RetryDelayElapsed,
            Self::AttemptSucceeded => TransferEventKind::AttemptSucceeded,
            Self::PermanentFailure(_) => TransferEventKind::PermanentFailure,
            Self::CancelRequested => TransferEventKind::CancelRequested,
            Self::CancellationFinished => TransferEventKind::CancellationFinished,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferEventKind {
    StartRequested,
    PauseRequested,
    ResumeRequested,
    ConflictDetected,
    ConflictResolved,
    RecoverableFailure,
    RetryDelayElapsed,
    AttemptSucceeded,
    PermanentFailure,
    CancelRequested,
    CancellationFinished,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferEffect {
    StartAttempt {
        attempt: AttemptNumber,
    },
    PauseAttempt {
        attempt: AttemptNumber,
    },
    ResumeAttempt {
        attempt: AttemptNumber,
    },
    RequestConflictDecision {
        attempt: AttemptNumber,
        conflict: ConflictKind,
    },
    ApplyConflictDecision {
        attempt: AttemptNumber,
        decision: ConflictDecision,
    },
    ScheduleRetry {
        attempt: AttemptNumber,
        last_failure: FailureKind,
    },
    CancelAttempt {
        attempt: AttemptNumber,
    },
    CancelRetry {
        attempt: AttemptNumber,
    },
    PublishTerminal {
        outcome: TerminalOutcome,
    },
}
