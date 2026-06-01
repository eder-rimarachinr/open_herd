mod daemon;

#[cfg(test)]
mod tests;

use tauri::Manager;
use daemon::{
    config::{resolve_base_dir, Config},
    server,
    state::AppState,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Resolve data directory (portable vs standard)
    let base_dir = resolve_base_dir();
    std::fs::create_dir_all(&base_dir).expect("Cannot create base dir");

    // Load or create config
    let config = Config::load(&base_dir);

    // Build shared application state
    let state = AppState::new(base_dir, config);

    // Start the HTTP API in a background Tokio runtime
    let state_for_api = state.clone();
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .expect("Failed to create Tokio runtime")
            .block_on(server::start(state_for_api));
    });

    // Launch Tauri
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            app.get_webview_window("main").unwrap().open_devtools();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
