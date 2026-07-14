// Modules are public so integration tests (tests/) can exercise them
// against in-process SSH/FTP servers.
pub mod app_config;
pub mod autolock;
pub mod commands;
pub mod error;
pub mod events;
pub mod local_fs;
pub mod runtime_context;
pub mod session;
pub mod state;
pub mod transfer;
pub mod vault;
pub mod watcher;

use tauri_specta::{collect_commands, collect_events, Builder};

fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::vault_get_info,
            commands::vault_create,
            commands::vault_unlock_password,
            commands::vault_unlock_quick,
            commands::vault_lock,
            commands::vault_change_password,
            commands::vault_set_touch_id,
            commands::connection_upsert,
            commands::connection_secrets,
            commands::connection_duplicate,
            commands::connection_delete,
            commands::folder_create,
            commands::folder_update,
            commands::folder_delete,
            commands::tree_update,
            commands::settings_update,
            commands::known_host_remove,
            commands::vault_set_path,
            commands::vault_switch_path,
            commands::session_connect,
            commands::session_disconnect,
            commands::host_key_accept,
            commands::term_open,
            commands::term_write,
            commands::term_resize,
            commands::term_close,
            commands::local_list,
            commands::local_home,
            commands::local_mkdir,
            commands::local_create_file,
            commands::local_rename,
            commands::local_delete,
            commands::local_chmod,
            commands::local_copy_into,
            commands::drag_preview_icon,
            commands::remote_list,
            commands::remote_home,
            commands::remote_mkdir,
            commands::remote_create_file,
            commands::remote_rename,
            commands::remote_delete,
            commands::remote_chmod,
            commands::s3_acl_status,
            commands::s3_set_acl,
            commands::s3_set_upload_acl,
            commands::transfer_upload,
            commands::transfer_download,
            commands::transfer_list,
            commands::transfer_pause,
            commands::transfer_resume,
            commands::transfer_cancel,
            commands::transfer_pause_all,
            commands::transfer_resume_all,
            commands::transfer_cancel_all,
            commands::transfer_clear_finished,
            commands::transfer_resolve,
            commands::transfer_retry,
            commands::remote_edit_open,
            commands::tunnel_start,
            commands::tunnel_stop,
            commands::tunnel_list,
            commands::vault_touch_activity,
            commands::vault_export_config,
            commands::vault_import_config,
            commands::ssh_key_read_file,
            commands::open_external,
        ])
        .events(collect_events![
            events::VaultLockedEvent,
            events::SessionStateEvent,
            events::TerminalDataEvent,
            events::TerminalExitEvent,
            events::TransferProgressEvent,
            events::RemoteEditUploadedEvent,
        ])
}

pub fn run() {
    let builder = specta_builder();

    #[cfg(debug_assertions)]
    builder
        .export(
            specta_typescript::Typescript::default(),
            "../src/lib/api/bindings.ts",
        )
        .expect("failed to export typescript bindings");

    tauri::Builder::default()
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::new())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            use tauri::Manager;
            builder.mount_events(app.app_handle());
            autolock::spawn(app.app_handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                // Temp copies of remote-edited files never outlive the app.
                watcher::cleanup_all();
            }
        });
}

#[cfg(test)]
mod tests {
    /// Regenerates TS bindings without launching the app:
    /// `cargo test export_bindings`.
    #[test]
    fn export_bindings() {
        super::specta_builder()
            .export(
                specta_typescript::Typescript::default(),
                "../src/lib/api/bindings.ts",
            )
            .expect("failed to export typescript bindings");
    }
}
