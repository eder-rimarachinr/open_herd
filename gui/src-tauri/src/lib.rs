use std::net::TcpStream;
use std::sync::Mutex;
use std::time::Duration;
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

/// Returns true if the daemon is already listening on port 7878.
fn is_daemon_running() -> bool {
    TcpStream::connect_timeout(
        &"127.0.0.1:7878".parse().unwrap(),
        Duration::from_millis(300),
    )
    .is_ok()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            app.get_webview_window("main").unwrap().open_devtools();

            // If the daemon is already running (e.g. left over from a previous
            // session or started manually), skip spawning a new sidecar.
            if is_daemon_running() {
                app.manage(DaemonState { child: Mutex::new(None) });
                return Ok(());
            }

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
            // Note: the daemon also auto-detects portable mode via its own exe
            // path, so this env var is belt-and-suspenders for the first launch
            // before the daemon self-elevates on Windows.
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
