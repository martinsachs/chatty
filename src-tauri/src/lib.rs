mod app;
pub mod backend;
pub mod config;
pub mod error;
pub mod model;
pub mod secrets;
pub mod store;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            app::setup(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app::app_info,
            app::set_settings,
            app::add_google_account,
            app::add_mattermost_account,
            app::remove_account,
            app::cached_spaces,
            app::list_spaces,
            app::cached_messages,
            app::list_messages,
            app::list_thread,
            app::send_message,
            app::edit_message,
            app::delete_message,
            app::react,
            app::download_attachment,
            app::search,
            app::mark_read,
            app::set_active,
            app::open_space_window,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
