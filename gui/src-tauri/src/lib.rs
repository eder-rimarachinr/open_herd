use std::sync::Mutex;
use tauri::Manager;

struct DaemonState {
    child: Mutex<Option<tauri_plugin_shell::process::CommandChild>>,
}

impl Drop for DaemonState {
    fn drop(&mut self) {
        if let Ok(mut lock) = self.child.lock() {
            if let Some(child) = lock.take() {
                let _ = child.kill();
            }
        }
    }
}

use tauri_plugin_shell::ShellExt;

/// Returns the portable data directory if the app is running in portable mode
/// (i.e. a `data/config.json` file exists next to the executable).
fn portable_data_dir() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let data_dir = exe.parent()?.join("data");
    if data_dir.join("config.json").exists() {
        Some(data_dir)
    } else {
        None
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            app.get_webview_window("main").unwrap().open_devtools();

            let sidecar = match app.shell().sidecar("phpenv-daemon") {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("phpenv-daemon sidecar not found: {}", e);
                    app.manage(DaemonState { child: Mutex::new(None) });
                    return Ok(());
                }
            };

            // In portable mode, tell the daemon where to store its data so it
            // uses the same directory as the GUI, not ~/.phpenv.
            let sidecar = if let Some(data_dir) = portable_data_dir() {
                sidecar.env("PHPENV_DATA_DIR", data_dir.to_string_lossy().as_ref())
            } else {
                sidecar
            };

            match sidecar.spawn() {
                Ok((_rx, child)) => {
                    app.manage(DaemonState { child: Mutex::new(Some(child)) });
                }
                Err(e) => {
                    eprintln!("Failed to spawn daemon: {}", e);
                    app.manage(DaemonState { child: Mutex::new(None) });
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
