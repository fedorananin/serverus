//! FTP/FTPS implementation of [`crate::session::remote_fs::RemoteFs`] (SPEC §4.3).
//!
//! FTP allows one transfer per control connection, so a pool of connections
//! backs parallel transfers. Metadata operations check a connection out and
//! return it; transfer streams own their connection until finalized.
//!
//! Recursive directory operations are implemented above this module through
//! the protocol-neutral trait and must always work.

mod adapter;
mod config;
mod listing;
mod pool;
mod streams;

pub use config::FtpConfig;
pub use pool::FtpPool;

#[cfg(test)]
mod tests;
