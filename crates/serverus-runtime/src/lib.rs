#![forbid(unsafe_code)]

//! Serverus runtime ownership, context generations, and supervisors.

pub mod context;

pub use context::{
    ApplicationHandle, ContextLease, RuntimeError, VaultAccessEpoch, VaultSwitchPermit,
};
