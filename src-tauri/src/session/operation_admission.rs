//! Context- and session-scoped admission for live remote operations.

mod registry;
mod runner;
mod state;

pub(super) use registry::SessionOperationRegistry;
