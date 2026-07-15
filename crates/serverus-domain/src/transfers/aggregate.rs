use std::fmt;

use super::{
    AttemptNumber, CompletionOutcome, ConflictDecision, RetryBudget, TerminalOutcome,
    TransferEffect, TransferEvent, TransferEventKind, TransferId, TransferState, TransferStateKind,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transfer {
    id: TransferId,
    retry_budget: RetryBudget,
    retries_used: u16,
    state: TransferState,
}

impl Transfer {
    pub const fn queued(id: TransferId, retry_budget: RetryBudget) -> Self {
        Self {
            id,
            retry_budget,
            retries_used: 0,
            state: TransferState::Queued,
        }
    }

    pub const fn id(&self) -> TransferId {
        self.id
    }

    pub const fn retry_budget(&self) -> RetryBudget {
        self.retry_budget
    }

    pub const fn retries_used(&self) -> u16 {
        self.retries_used
    }

    pub const fn state(&self) -> &TransferState {
        &self.state
    }

    pub fn transition(&self, event: TransferEvent) -> Result<Transition, InvalidTransition> {
        let event_kind = event.kind();
        match (&self.state, event) {
            (TransferState::Queued, TransferEvent::StartRequested) => {
                let attempt = AttemptNumber::first();
                Ok(Transition {
                    next: Self {
                        state: TransferState::Running { attempt },
                        ..self.clone()
                    },
                    effects: vec![TransferEffect::StartAttempt { attempt }],
                })
            }
            (TransferState::Running { attempt }, TransferEvent::PauseRequested) => Ok(Transition {
                next: Self {
                    state: TransferState::Paused { attempt: *attempt },
                    ..self.clone()
                },
                effects: vec![TransferEffect::PauseAttempt { attempt: *attempt }],
            }),
            (TransferState::Paused { attempt }, TransferEvent::ResumeRequested) => Ok(Transition {
                next: Self {
                    state: TransferState::Running { attempt: *attempt },
                    ..self.clone()
                },
                effects: vec![TransferEffect::ResumeAttempt { attempt: *attempt }],
            }),
            (TransferState::Running { attempt }, TransferEvent::ConflictDetected(conflict)) => {
                Ok(Transition {
                    next: Self {
                        state: TransferState::WaitingForConflict {
                            attempt: *attempt,
                            conflict,
                        },
                        ..self.clone()
                    },
                    effects: vec![TransferEffect::RequestConflictDecision {
                        attempt: *attempt,
                        conflict,
                    }],
                })
            }
            (
                TransferState::WaitingForConflict { attempt, .. },
                TransferEvent::ConflictResolved(
                    decision @ (ConflictDecision::Overwrite | ConflictDecision::Rename),
                ),
            ) => Ok(Transition {
                next: Self {
                    state: TransferState::Running { attempt: *attempt },
                    ..self.clone()
                },
                effects: vec![TransferEffect::ApplyConflictDecision {
                    attempt: *attempt,
                    decision,
                }],
            }),
            (
                TransferState::WaitingForConflict { attempt, .. },
                TransferEvent::ConflictResolved(ConflictDecision::Skip),
            ) => {
                let outcome = CompletionOutcome::Skipped;
                Ok(Transition {
                    next: Self {
                        state: TransferState::Completed { outcome },
                        ..self.clone()
                    },
                    effects: vec![
                        TransferEffect::ApplyConflictDecision {
                            attempt: *attempt,
                            decision: ConflictDecision::Skip,
                        },
                        TransferEffect::PublishTerminal {
                            outcome: TerminalOutcome::Completed(outcome),
                        },
                    ],
                })
            }
            (TransferState::Running { attempt }, TransferEvent::RecoverableFailure(failure))
                if self.retries_used < self.retry_budget.max_retries() =>
            {
                let next_attempt = attempt.next();
                Ok(Transition {
                    next: Self {
                        retries_used: self.retries_used + 1,
                        state: TransferState::WaitingForRetry {
                            attempt: next_attempt,
                            last_failure: failure,
                        },
                        ..self.clone()
                    },
                    effects: vec![TransferEffect::ScheduleRetry {
                        attempt: next_attempt,
                        last_failure: failure,
                    }],
                })
            }
            (TransferState::Running { .. }, TransferEvent::RecoverableFailure(failure)) => {
                Ok(Transition {
                    next: Self {
                        state: TransferState::Failed { failure },
                        ..self.clone()
                    },
                    effects: vec![TransferEffect::PublishTerminal {
                        outcome: TerminalOutcome::Failed(failure),
                    }],
                })
            }
            (TransferState::WaitingForRetry { attempt, .. }, TransferEvent::RetryDelayElapsed) => {
                Ok(Transition {
                    next: Self {
                        state: TransferState::Running { attempt: *attempt },
                        ..self.clone()
                    },
                    effects: vec![TransferEffect::StartAttempt { attempt: *attempt }],
                })
            }
            (TransferState::Running { .. }, TransferEvent::AttemptSucceeded) => {
                let outcome = CompletionOutcome::Transferred;
                Ok(Transition {
                    next: Self {
                        state: TransferState::Completed { outcome },
                        ..self.clone()
                    },
                    effects: vec![TransferEffect::PublishTerminal {
                        outcome: TerminalOutcome::Completed(outcome),
                    }],
                })
            }
            (TransferState::Running { .. }, TransferEvent::PermanentFailure(failure)) => {
                Ok(Transition {
                    next: Self {
                        state: TransferState::Failed { failure },
                        ..self.clone()
                    },
                    effects: vec![TransferEffect::PublishTerminal {
                        outcome: TerminalOutcome::Failed(failure),
                    }],
                })
            }
            (TransferState::Queued, TransferEvent::CancelRequested) => Ok(Transition {
                next: Self {
                    state: TransferState::Cancelled,
                    ..self.clone()
                },
                effects: vec![TransferEffect::PublishTerminal {
                    outcome: TerminalOutcome::Cancelled,
                }],
            }),
            (
                TransferState::Running { attempt }
                | TransferState::Paused { attempt }
                | TransferState::WaitingForConflict { attempt, .. },
                TransferEvent::CancelRequested,
            ) => Ok(Transition {
                next: Self {
                    state: TransferState::Cancelling { attempt: *attempt },
                    ..self.clone()
                },
                effects: vec![TransferEffect::CancelAttempt { attempt: *attempt }],
            }),
            (TransferState::WaitingForRetry { attempt, .. }, TransferEvent::CancelRequested) => {
                Ok(Transition {
                    next: Self {
                        state: TransferState::Cancelled,
                        ..self.clone()
                    },
                    effects: vec![
                        TransferEffect::CancelRetry { attempt: *attempt },
                        TransferEffect::PublishTerminal {
                            outcome: TerminalOutcome::Cancelled,
                        },
                    ],
                })
            }
            (TransferState::Cancelling { .. }, TransferEvent::CancellationFinished) => {
                Ok(Transition {
                    next: Self {
                        state: TransferState::Cancelled,
                        ..self.clone()
                    },
                    effects: vec![TransferEffect::PublishTerminal {
                        outcome: TerminalOutcome::Cancelled,
                    }],
                })
            }
            (
                TransferState::Cancelling { .. }
                | TransferState::Completed { .. }
                | TransferState::Cancelled
                | TransferState::Failed { .. },
                TransferEvent::CancelRequested,
            ) => Ok(Transition {
                next: self.clone(),
                effects: Vec::new(),
            }),
            _ => Err(InvalidTransition {
                from: self.state.kind(),
                event: event_kind,
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transition {
    pub next: Transfer,
    pub effects: Vec<TransferEffect>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidTransition {
    from: TransferStateKind,
    event: TransferEventKind,
}

impl InvalidTransition {
    pub const fn from(self) -> TransferStateKind {
        self.from
    }

    pub const fn event(self) -> TransferEventKind {
        self.event
    }
}

impl fmt::Display for InvalidTransition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "event {:?} is invalid while transfer is {:?}",
            self.event, self.from
        )
    }
}

impl std::error::Error for InvalidTransition {}
