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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            app.get_webview_window("main").unwrap().open_devtools();

            // Spawn daemon using the official sidecar API
            // The name must match the path in tauri.conf.json (without the triple suffix)
            let sidecar = app.shell().sidecar("bin/phpenv-daemon").unwrap();
            let (mut _rx, child) = sidecar.spawn().unwrap();

            app.manage(DaemonState {
                child: Mutex::new(Some(child)),
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
