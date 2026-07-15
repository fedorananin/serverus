#![forbid(unsafe_code)]

//! Reusable deterministic fakes and contract suites for Serverus tests.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serverus_application::context::{
    AppEventSink, ContextCleanup, ContextCleanupError, ContextEvent, RuntimeContextIdGenerator,
};
use serverus_domain::runtime_context::{InvalidRuntimeContextId, RuntimeContextId};

/// A finite ID source that makes context generations explicit in tests.
#[derive(Clone, Debug)]
pub struct DeterministicContextIds {
    remaining: Arc<Mutex<VecDeque<RuntimeContextId>>>,
}

impl DeterministicContextIds {
    pub fn new(values: impl IntoIterator<Item = u128>) -> Result<Self, InvalidRuntimeContextId> {
        let remaining = values
            .into_iter()
            .map(RuntimeContextId::try_from)
            .collect::<Result<VecDeque<_>, _>>()?;
        Ok(Self {
            remaining: Arc::new(Mutex::new(remaining)),
        })
    }
}

impl RuntimeContextIdGenerator for DeterministicContextIds {
    fn next_id(&self) -> RuntimeContextId {
        self.remaining
            .lock()
            .expect("deterministic ID mutex is poisoned")
            .pop_front()
            .expect("deterministic context ID sequence is exhausted")
    }
}

#[derive(Default)]
struct CleanupState {
    retired: Vec<RuntimeContextId>,
    next_failure: Option<String>,
}

/// Records cleanup calls and can inject one deterministic failure.
#[derive(Clone, Default)]
pub struct RecordedContextCleanup {
    state: Arc<Mutex<CleanupState>>,
}

impl RecordedContextCleanup {
    pub fn fail_next(&self, message: impl Into<String>) {
        self.state
            .lock()
            .expect("cleanup mutex is poisoned")
            .next_failure = Some(message.into());
    }

    pub fn snapshot(&self) -> Vec<RuntimeContextId> {
        self.state
            .lock()
            .expect("cleanup mutex is poisoned")
            .retired
            .clone()
    }
}

#[async_trait]
impl ContextCleanup for RecordedContextCleanup {
    async fn retire(&self, context_id: RuntimeContextId) -> Result<(), ContextCleanupError> {
        let failure = {
            let mut state = self.state.lock().expect("cleanup mutex is poisoned");
            state.retired.push(context_id);
            state.next_failure.take()
        };
        match failure {
            Some(message) => Err(ContextCleanupError::new(message)),
            None => Ok(()),
        }
    }
}

/// In-memory ordered event sink for application/runtime tests.
#[derive(Clone, Default)]
pub struct RecordedContextEvents {
    events: Arc<Mutex<Vec<ContextEvent>>>,
}

impl RecordedContextEvents {
    pub fn snapshot(&self) -> Vec<ContextEvent> {
        self.events.lock().expect("event mutex is poisoned").clone()
    }
}

impl AppEventSink for RecordedContextEvents {
    fn publish(&self, event: ContextEvent) {
        self.events
            .lock()
            .expect("event mutex is poisoned")
            .push(event);
    }
}
