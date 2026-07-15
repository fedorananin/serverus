//! Admission boundary for context-owned session connection attempts.

mod registry;
mod runner;
mod state;

pub(super) use registry::ConnectAdmissionRegistry;
