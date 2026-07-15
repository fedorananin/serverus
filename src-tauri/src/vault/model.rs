//! Vault payload data model (SPEC §3, §8).
//!
//! Everything here is serialized into the encrypted vault payload. Types that
//! cross the IPC boundary have redacted public counterparts; secrets never
//! leave the backend.

mod connection;
mod input;
mod payload;
mod public;
mod settings;
mod tree;

pub use connection::*;
pub use input::*;
pub use payload::*;
pub use public::*;
pub use settings::*;
pub use tree::*;

#[cfg(test)]
mod tests;
