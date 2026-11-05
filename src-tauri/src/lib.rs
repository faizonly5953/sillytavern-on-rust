//! SillyTavern on Rust: everything the old Express server did now runs
//! in-process and only while the app window is open (no persistent server).

mod avatar_protocol;
mod character;
mod chat;
mod commands;
mod error;
mod macros;
mod paths;
mod png_meta;
mod prompt;
mod providers;
mod settings;
mod state;
mod updates;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());
    // In-place self-update exists only on desktop; Android opens the new APK instead.
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init());

    builder
        .register_asynchronous_uri_scheme_protocol("avatar", avatar_protocol::handle)
        .setup(|app| {
            // SILLYTAVERN_RS_DATA lets desktop users keep data off the system drive.
            let root = match std::env::var_os("SILLYTAVERN_RS_DATA") {
                Some(dir) if !dir.is_empty() => std::path::PathBuf::from(dir),
                _ => app.path().app_data_dir()?,
            };
            app.manage(state::AppState::new(root)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::get_data_dir,
            commands::get_secret_status,
            commands::set_secret,
            commands::set_persona_avatar,
            commands::list_characters,
            commands::get_character,
            commands::create_character,
            commands::update_character,
            commands::set_favorite,
            commands::delete_character,
            commands::import_character,
            commands::list_chats,
            commands::load_chat,
            commands::create_chat,
            commands::save_chat,
            commands::delete_chat,
            commands::rename_chat,
            commands::generate,
            commands::stop_generation,
            commands::preview_prompt,
            commands::list_models,
            commands::app_info,
            commands::check_release,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
