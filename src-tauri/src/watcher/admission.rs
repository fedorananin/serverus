//! Admission boundary for runtime-owned remote-edit opens.

mod lifecycle;
mod registry;
mod runner;
mod state;

pub(super) use registry::OpenAdmissionRegistry;
