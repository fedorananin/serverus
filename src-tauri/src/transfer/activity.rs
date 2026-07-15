//! Admission and task ownership for context-scoped transfer activity.

mod admission;
mod queue;
mod registry;
mod state;
mod tasks;

pub(super) use queue::ServerQueue;
pub(super) use registry::ActivityRegistry;
pub(super) use state::AdmissionToken;
