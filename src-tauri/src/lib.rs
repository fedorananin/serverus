// Modules are public so integration tests (tests/) can exercise them
// against in-process SSH/FTP servers.
pub mod app_config;
pub mod autolock;
pub mod commands;
pub mod error;
pub mod events;
pub mod local_fs;
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
            commands::sessions::connect::session_connect,
            commands::sessions::lifecycle::session_disconnect,
            commands::sessions::host_key::host_key_accept,
            commands::sessions::terminal::term_open,
            commands::sessions::terminal::term_write,
            commands::sessions::terminal::term_resize,
            commands::sessions::terminal::term_close,
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
            commands::remote_edit_notifications,
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
            events::TransferProgressEvent,
            events::RemoteEditUploadedEvent,
        ])
}

/// Generate the TypeScript IPC bindings at their single committed location.
///
/// This is intentionally called only by the dedicated `generate-bindings`
/// binary. App startup and ordinary tests must not mutate tracked sources.
pub fn generate_typescript_bindings() -> Result<(), String> {
    let output = concat!(env!("CARGO_MANIFEST_DIR"), "/../src/lib/api/bindings.ts");
    specta_builder()
        .export(specta_typescript::Typescript::default(), output)
        .map_err(|error| format!("failed to generate TypeScript bindings: {error}"))
}

pub fn run() {
    let builder = specta_builder();
    let tauri_builder = tauri::Builder::default();

    #[cfg(feature = "scenario-tests")]
    let tauri_builder = tauri_builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    tauri_builder
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
