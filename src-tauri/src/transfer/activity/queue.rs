use std::sync::Arc;

use tokio::sync::Semaphore;

pub(in crate::transfer) struct ServerQueue {
    pub(in crate::transfer) semaphore: Arc<Semaphore>,
}
