//! Session command facade, split by workflow responsibility.

pub(crate) mod connect;
pub(crate) mod host_key;
pub(crate) mod lifecycle;
pub(crate) mod terminal;

pub use connect::session_connect;
pub use host_key::host_key_accept;
pub use lifecycle::session_disconnect;
pub use terminal::{term_close, term_open, term_resize, term_write};

#[cfg(test)]
use host_key::accept_host_key_for_context;

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
