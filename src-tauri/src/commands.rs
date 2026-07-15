//! Tauri command facade.
//!
//! Command groups live in `commands/`; this module preserves the public
//! `commands::name` surface used by Tauri, Specta, and existing call sites.

mod helpers;
mod local_copy;
mod local_files;
mod misc;
mod prelude;
mod remote_edit;
mod remote_files;
mod s3;
pub(crate) mod sessions;
mod transfers;
mod tunnels;
mod types;
mod vault_access;
mod vault_io;
mod vault_location;
mod vault_tree;

pub use local_copy::*;
pub use local_files::*;
pub use misc::*;
pub use remote_edit::*;
pub use remote_files::*;
pub use s3::*;
pub use sessions::*;
pub use transfers::*;
pub use tunnels::*;
pub use types::*;
pub use vault_access::*;
pub use vault_io::*;
pub use vault_location::*;
pub use vault_tree::*;

#[cfg(test)]
#[path = "commands/vault_context_tests.rs"]
mod vault_context_tests;
