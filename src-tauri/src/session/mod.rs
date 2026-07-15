//! Session manager: registry of live connections and terminal channels
//! (SPEC §7.1). One SSH session multiplexes terminals, SFTP and tunnels.

mod admission;
mod connection;
mod entry;
mod manager;
mod operation_admission;
mod resource_cleanup;
mod terminal;
mod terminal_stream;

pub mod ftp;
pub mod remote_fs;
pub mod s3;
pub mod sftp;
pub mod ssh;
pub mod tunnel;

pub(crate) use connection::load_authorized_plan;
pub use entry::SessionEntry;
pub use manager::SessionManager;
pub(crate) use resource_cleanup::SessionResourceCleanup;
pub use terminal_stream::TerminalStreamEvent;

#[cfg(test)]
mod tests;
