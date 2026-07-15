use crate::events::TransferProgressEvent;

/// Where progress events go. Tests use a channel instead of a Tauri runtime.
pub trait ProgressSink: Send + Sync + 'static {
    fn emit(&self, event: TransferProgressEvent);
}

impl ProgressSink for tauri::AppHandle {
    fn emit(&self, event: TransferProgressEvent) {
        let _ = tauri_specta::Event::emit(&event, self);
    }
}
