//! Global boundary between the selected vault and runtime resources derived
//! from it. Even epochs accept commands; odd epochs are fail-closed while a
//! vault switch is draining the previous context.

use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::error::{AppError, AppResult};

pub struct RuntimeContext {
    epoch: AtomicU64,
    switch_lock: tokio::sync::Mutex<()>,
    next_connect_id: AtomicU64,
    pending_connects: Mutex<HashMap<u64, tokio::sync::watch::Sender<bool>>>,
    connect_finished: tokio::sync::Notify,
}

impl Default for RuntimeContext {
    fn default() -> Self {
        Self {
            epoch: AtomicU64::new(0),
            switch_lock: tokio::sync::Mutex::new(()),
            next_connect_id: AtomicU64::new(0),
            pending_connects: Mutex::new(HashMap::new()),
            connect_finished: tokio::sync::Notify::new(),
        }
    }
}

impl RuntimeContext {
    pub fn current_epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    pub async fn lock_expected(
        &self,
        expected_epoch: u64,
    ) -> AppResult<tokio::sync::MutexGuard<'_, ()>> {
        let guard = self.switch_lock.lock().await;
        if !expected_epoch.is_multiple_of(2) || self.current_epoch() != expected_epoch {
            return Err(AppError::VaultContextClosed);
        }
        Ok(guard)
    }

    pub async fn lock_current(&self) -> AppResult<tokio::sync::MutexGuard<'_, ()>> {
        let expected = self.current_epoch();
        self.lock_expected(expected).await
    }

    pub async fn lock_switch(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.switch_lock.lock().await
    }

    /// The only vault-scoped operation allowed to outlive the switch mutex is
    /// connection setup, which may wait on a network timeout. Register it
    /// before releasing the mutex so a switch can cancel and drain it without
    /// a late-session race.
    pub async fn run_pending_connect<T, E>(
        self: &Arc<Self>,
        expected_epoch: u64,
        future: impl Future<Output = Result<T, E>>,
    ) -> Result<T, E>
    where
        E: From<AppError>,
    {
        let _switch = self.lock_expected(expected_epoch).await.map_err(E::from)?;
        let id = self.next_connect_id.fetch_add(1, Ordering::Relaxed);
        let (cancel, mut cancelled) = tokio::sync::watch::channel(false);
        self.pending_connects.lock().unwrap().insert(id, cancel);
        drop(_switch);
        let _registration = PendingConnect {
            id,
            context: self.clone(),
        };
        tokio::select! {
            result = future => result,
            _ = async {
                if !*cancelled.borrow() {
                    let _ = cancelled.changed().await;
                }
            } => Err(AppError::VaultContextClosed.into()),
        }
    }

    /// Called only while the switch mutex is held and the epoch is odd.
    pub async fn cancel_pending_connects(&self) {
        loop {
            let finished = self.connect_finished.notified();
            let empty = {
                let pending = self.pending_connects.lock().unwrap();
                for cancel in pending.values() {
                    let _ = cancel.send(true);
                }
                pending.is_empty()
            };
            if empty {
                return;
            }
            finished.await;
        }
    }

    /// Begin a switch for the caller-observed even epoch, or resume the same
    /// fail-closed switch after a teardown/commit error.
    pub fn begin_or_resume_switch(
        self: &Arc<Self>,
        expected_epoch: u64,
    ) -> AppResult<RuntimeSwitchGuard> {
        if !expected_epoch.is_multiple_of(2) {
            return Err(AppError::VaultContextClosed);
        }
        let odd_epoch = expected_epoch + 1;
        match self.current_epoch() {
            current if current == expected_epoch => {
                self.epoch
                    .compare_exchange(
                        expected_epoch,
                        odd_epoch,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .map_err(|_| AppError::VaultContextClosed)?;
            }
            current if current == odd_epoch => {}
            _ => {
                return Err(AppError::VaultContextClosed);
            }
        }
        Ok(RuntimeSwitchGuard {
            context: self.clone(),
            odd_epoch,
        })
    }
}

struct PendingConnect {
    id: u64,
    context: Arc<RuntimeContext>,
}

impl Drop for PendingConnect {
    fn drop(&mut self) {
        self.context
            .pending_connects
            .lock()
            .unwrap()
            .remove(&self.id);
        // Store a permit if the switch has not reached its await yet; using
        // notify_waiters here would allow a drop-between-check-and-await hang.
        self.context.connect_finished.notify_one();
    }
}

/// Dropping an unfinished switch intentionally leaves the epoch odd. Only a
/// fully drained and durably committed replacement may reopen commands.
pub struct RuntimeSwitchGuard {
    context: Arc<RuntimeContext>,
    odd_epoch: u64,
}

impl RuntimeSwitchGuard {
    pub fn finish(self) {
        self.context
            .epoch
            .compare_exchange(
                self.odd_epoch,
                self.odd_epoch + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .expect("vault context switch guard mismatch");
    }
}

#[cfg(test)]
mod tests {
    use super::RuntimeContext;
    use crate::error::{AppError, AppResult};
    use std::sync::atomic::Ordering;
    use std::sync::Arc;

    #[tokio::test]
    async fn failed_switch_stays_closed_and_can_be_resumed() {
        let context = Arc::new(RuntimeContext::default());
        let switch = context.begin_or_resume_switch(0).unwrap();
        drop(switch);

        assert_eq!(context.current_epoch(), 1);
        assert!(context.lock_expected(0).await.is_err());

        context.begin_or_resume_switch(0).unwrap().finish();
        assert_eq!(context.current_epoch(), 2);
    }

    #[tokio::test]
    async fn old_epoch_is_rejected_after_a_completed_switch() {
        let context = Arc::new(RuntimeContext::default());
        context.begin_or_resume_switch(0).unwrap().finish();

        assert!(context.lock_expected(0).await.is_err());
        assert!(context.lock_expected(2).await.is_ok());
    }

    #[tokio::test]
    async fn switch_cancels_and_drops_a_pending_connect_before_drain() {
        struct DropSignal(Option<tokio::sync::oneshot::Sender<()>>);
        impl Drop for DropSignal {
            fn drop(&mut self) {
                if let Some(signal) = self.0.take() {
                    let _ = signal.send(());
                }
            }
        }

        let context = Arc::new(RuntimeContext::default());
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (dropped_tx, dropped_rx) = tokio::sync::oneshot::channel();
        let connect = tokio::spawn({
            let context = context.clone();
            async move {
                context
                    .run_pending_connect::<(), AppError>(0, async move {
                        let _drop = DropSignal(Some(dropped_tx));
                        let _ = started_tx.send(());
                        std::future::pending::<AppResult<()>>().await
                    })
                    .await
            }
        });
        started_rx.await.unwrap();

        let _switch_lock = context.lock_switch().await;
        let switch = context.begin_or_resume_switch(0).unwrap();
        context.cancel_pending_connects().await;

        assert!(connect.await.unwrap().is_err());
        dropped_rx.await.unwrap();
        switch.finish();
    }

    #[tokio::test]
    async fn connect_cannot_register_after_the_epoch_closes() {
        let context = Arc::new(RuntimeContext::default());
        let _switch_lock = context.lock_switch().await;
        let switch = context.begin_or_resume_switch(0).unwrap();

        // Release the mutex only after the odd epoch is visible; the delayed
        // setup must fail before polling its future.
        drop(_switch_lock);
        let polled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let future_polled = polled.clone();
        let result = context
            .run_pending_connect::<(), AppError>(0, async move {
                future_polled.store(true, Ordering::SeqCst);
                Ok(())
            })
            .await;

        assert!(result.is_err());
        assert!(!polled.load(Ordering::SeqCst));
        switch.finish();
    }
}
