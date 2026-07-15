use std::sync::Arc;

use tokio::task::AbortHandle;

use super::registry::ActivityRegistry;
use super::state::{AdmissionToken, TaskActivity};

impl ActivityRegistry {
    pub(in crate::transfer) fn reserve_task(
        self: &Arc<Self>,
        admission: &AdmissionToken,
    ) -> Option<TaskGuard> {
        let mut state = self.state.lock().unwrap();
        if !Self::token_is_active(&state, admission) {
            return None;
        }
        let id = state.next_task_id;
        state.next_task_id = state.next_task_id.wrapping_add(1).max(1);
        state.tasks.insert(
            id,
            TaskActivity {
                admission: admission.clone(),
                abort: None,
            },
        );
        Some(TaskGuard {
            registry: self.clone(),
            id,
        })
    }

    pub(in crate::transfer) fn attach_abort(&self, id: u64, abort: AbortHandle) {
        let should_abort = {
            let mut state = self.state.lock().unwrap();
            let active = state
                .tasks
                .get(&id)
                .is_some_and(|task| Self::token_is_active(&state, &task.admission));
            if let Some(task) = state.tasks.get_mut(&id) {
                task.abort = Some(abort.clone());
            } else {
                return;
            }
            !active
        };
        if should_abort {
            abort.abort();
        }
    }
}

pub(in crate::transfer) struct TaskGuard {
    registry: Arc<ActivityRegistry>,
    id: u64,
}

impl TaskGuard {
    pub(in crate::transfer) fn id(&self) -> u64 {
        self.id
    }
}

impl Drop for TaskGuard {
    fn drop(&mut self) {
        self.registry.state.lock().unwrap().tasks.remove(&self.id);
        self.registry.changed.notify_waiters();
    }
}
