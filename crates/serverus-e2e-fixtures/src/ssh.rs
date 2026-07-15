#[cfg(not(unix))]
mod unavailable;
#[cfg(unix)]
mod unix;

#[cfg(not(unix))]
pub use unavailable::SshServer;
#[cfg(unix)]
pub use unix::SshServer;
