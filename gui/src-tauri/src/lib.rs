use std::sync::{Arc, Mutex};
use tauri::Manager;

struct DaemonState {
    child: Mutex<Option<std::process::Child>>,
}

impl Drop for DaemonState {
    fn drop(&mut self) {
        if let Ok(mut lock) = self.child.lock() {
            if let Some(mut child) = lock.take() {
                let _ = child.kill();
            }
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            app.get_webview_window("main").unwrap().open_devtools();

            // Spawn daemon and keep its handle in managed state so it's killed on exit
            let daemon_child = spawn_daemon(app.handle());
            app.manage(DaemonState {
                child: Mutex::new(daemon_child),
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Tries to find and start phpenv-daemon. Returns None if not found
/// (e.g. first run in dev before the Go binary is compiled).
fn spawn_daemon(app: &tauri::AppHandle) -> Option<std::process::Child> {
    let daemon_bin = find_daemon_binary(app);

    let bin = match daemon_bin {
        Some(b) => b,
        None => {
            eprintln!("[phpenv] daemon binary not found — start it manually with: go run ./daemon");
            return None;
        }
    };

    match std::process::Command::new(&bin)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(child) => {
            println!("[phpenv] daemon started (pid {})", child.id());
            Some(child)
        }
        Err(e) => {
            eprintln!("[phpenv] could not start daemon: {e}");
            None
        }
    }
}

fn find_daemon_binary(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    // 1. Bundled sidecar next to the app binary (production).
    if let Ok(resource_dir) = app.path().resource_dir() {
        let sidecar = resource_dir.join(daemon_exe());
        if sidecar.exists() {
            return Some(sidecar);
        }
    }

    // 2. ~/.phpenv/bin/ (installed via install.sh / install.ps1).
    if let Some(home) = dirs_next::home_dir() {
        let installed = home.join(".phpenv").join("bin").join(daemon_exe());
        if installed.exists() {
            return Some(installed);
        }
    }

    // 3. PATH.
    which::which(daemon_exe()).ok()
}

fn daemon_exe() -> &'static str {
    if cfg!(windows) { "phpenv-daemon.exe" } else { "phpenv-daemon" }
}
