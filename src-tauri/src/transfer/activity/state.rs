use std::collections::HashMap;

use serverus_domain::runtime_context::RuntimeContextId;
use tokio::task::AbortHandle;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::transfer) struct AdmissionToken {
    pub(in crate::transfer) context_id: RuntimeContextId,
    pub(in crate::transfer) context_generation: u64,
    pub(in crate::transfer) session_generation: u64,
    pub(in crate::transfer) session_id: String,
}

pub(in crate::transfer) struct SessionActivity {
    pub(in crate::transfer) generation: u64,
    pub(in crate::transfer) accepting: bool,
    pub(in crate::transfer) producers: usize,
}

impl Default for SessionActivity {
    fn default() -> Self {
        Self {
            generation: 0,
            accepting: true,
            producers: 0,
        }
    }
}

pub(in crate::transfer) struct TaskActivity {
    pub(in crate::transfer) admission: AdmissionToken,
    pub(in crate::transfer) abort: Option<AbortHandle>,
}

pub(in crate::transfer) struct ActivityState {
    pub(in crate::transfer) context_generation: u64,
    pub(in crate::transfer) active_context_id: Option<RuntimeContextId>,
    pub(in crate::transfer) accepting: bool,
    pub(in crate::transfer) sessions: HashMap<String, SessionActivity>,
    pub(in crate::transfer) tasks: HashMap<u64, TaskActivity>,
    pub(in crate::transfer) next_task_id: u64,
}

impl Default for ActivityState {
    fn default() -> Self {
        Self {
            context_generation: 0,
            active_context_id: None,
            accepting: true,
            sessions: HashMap::new(),
            tasks: HashMap::new(),
            next_task_id: 1,
        }
    }
}
