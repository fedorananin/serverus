//! Remote edit (SPEC §5.3): download a remote file into an isolated temp
//! directory, open it in the user's editor, and upload saves automatically.

mod admission;
mod cache;
mod editor;
mod lifecycle;
mod notifications;
mod open;
mod types;
mod upload;

use std::collections::HashMap;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use admission::OpenAdmissionRegistry;
pub use cache::cleanup_all;
use types::WatchedFile;

use crate::events::RemoteEditUploadedEvent;

#[derive(Default)]
pub struct EditWatcher {
    files: Mutex<HashMap<PathBuf, WatchedFile>>,
    notifications: Arc<Mutex<VecDeque<RemoteEditUploadedEvent>>>,
    admissions: Arc<OpenAdmissionRegistry>,
}

#[cfg(test)]
mod tests;
