use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serverus_application::context::{
    AppEventSink, ContextCleanup, ContextCleanupError, ContextEvent, RuntimeContextIdGenerator,
};
use serverus_domain::runtime_context::RuntimeContextId;
use serverus_runtime::ApplicationHandle;
use tokio::sync::Notify;

pub(crate) struct FixedId;

impl RuntimeContextIdGenerator for FixedId {
    fn next_id(&self) -> RuntimeContextId {
        RuntimeContextId::try_from(11_u128).unwrap()
    }
}

pub(crate) struct NoopCleanup;

#[async_trait]
impl ContextCleanup for NoopCleanup {
    async fn retire(&self, _context_id: RuntimeContextId) -> Result<(), ContextCleanupError> {
        Ok(())
    }
}

#[derive(Clone, Default)]
pub(crate) struct RecordedCleanup(Arc<Mutex<Vec<RuntimeContextId>>>);

#[async_trait]
impl ContextCleanup for RecordedCleanup {
    async fn retire(&self, context_id: RuntimeContextId) -> Result<(), ContextCleanupError> {
        self.0.lock().unwrap().push(context_id);
        Ok(())
    }
}

impl RecordedCleanup {
    pub(crate) fn snapshot(&self) -> Vec<RuntimeContextId> {
        self.0.lock().unwrap().clone()
    }
}

#[derive(Clone, Default)]
pub(crate) struct GatedCleanup {
    pub(crate) started: Arc<Notify>,
    pub(crate) release: Arc<Notify>,
}

#[async_trait]
impl ContextCleanup for GatedCleanup {
    async fn retire(&self, _context_id: RuntimeContextId) -> Result<(), ContextCleanupError> {
        self.started.notify_one();
        self.release.notified().await;
        Ok(())
    }
}

#[derive(Clone, Default)]
pub(crate) struct PanickingGatedCleanup {
    pub(crate) started: Arc<Notify>,
    pub(crate) release: Arc<Notify>,
}

#[async_trait]
impl ContextCleanup for PanickingGatedCleanup {
    async fn retire(&self, _context_id: RuntimeContextId) -> Result<(), ContextCleanupError> {
        self.started.notify_one();
        self.release.notified().await;
        panic!("simulated cleanup panic");
    }
}

pub(crate) struct NoopEvents;

impl AppEventSink for NoopEvents {
    fn publish(&self, _event: ContextEvent) {}
}

#[derive(Clone, Default)]
pub(crate) struct RecordedEvents(Arc<Mutex<Vec<ContextEvent>>>);

impl AppEventSink for RecordedEvents {
    fn publish(&self, event: ContextEvent) {
        self.0.lock().unwrap().push(event);
    }
}

impl RecordedEvents {
    pub(crate) fn snapshot(&self) -> Vec<ContextEvent> {
        self.0.lock().unwrap().clone()
    }
}

pub(crate) fn handle() -> ApplicationHandle {
    ApplicationHandle::new(
        Arc::new(FixedId),
        Arc::new(NoopCleanup),
        Arc::new(NoopEvents),
    )
}
