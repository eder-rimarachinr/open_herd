pub mod application;
pub mod daemon;
pub mod domain;
pub mod infrastructure;
pub mod ports;

use daemon::{
    config::{resolve_base_dir, Config},
    server,
    state::AppState,
};
use infrastructure::container::AppContainer;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let base_dir = resolve_base_dir();
    std::fs::create_dir_all(&base_dir).expect("Cannot create base dir");

    let config = Config::load(&base_dir);
    let api_addr = config.api_addr.clone();

    // ── Gracefully shut down any stale daemon from a previous session ──────
    // During `tauri dev` hot-reloads the old Rust binary is killed but the OS
    // may keep the port open for a few seconds (TIME_WAIT / lingering socket).
    // Sending /daemon/quit first lets the previous instance close cleanly and
    // frees the port before we try to bind.
    try_shutdown_previous(&api_addr);

    let state = AppState::new(base_dir, config);
    let container = AppContainer::new(state);

    // Start HTTP API in a background Tokio runtime
    let container_for_api = container.clone();
    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .expect("Failed to create Tokio runtime")
            .block_on(server::start(container_for_api));
    });

    // Launch Tauri
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            #[cfg(debug_assertions)]
            {
                use tauri::Manager;
                app.get_webview_window("main").unwrap().open_devtools();
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Try to tell a previous daemon instance to quit, then wait for the port to
/// become free (up to ~3 seconds).  Silently ignores any errors — if there is
/// no previous instance this is a no-op.
fn try_shutdown_previous(api_addr: &str) {
    let url = format!("http://{}/api/v1/daemon/quit", api_addr);

    // Best-effort POST — ignore errors (daemon may not be running)
    let _ = std::process::Command::new("curl")
        .args(["-s", "-X", "POST", "--max-time", "1", &url])
        .output();

    // Alternatively use a raw TCP connect to check and a simple HTTP POST.
    // This works without curl on all platforms:
    if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
        &api_addr.parse().unwrap_or("127.0.0.1:7878".parse().unwrap()),
        std::time::Duration::from_millis(300),
    ) {
        use std::io::Write;
        let req = format!(
            "POST /api/v1/daemon/quit HTTP/1.0\r\nHost: {}\r\nContent-Length: 0\r\n\r\n",
            api_addr
        );
        let _ = stream.write_all(req.as_bytes());
        drop(stream);
    }

    // Give the previous process up to 3 s to exit and release the port
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        if std::net::TcpStream::connect_timeout(
            &api_addr.parse().unwrap_or("127.0.0.1:7878".parse().unwrap()),
            std::time::Duration::from_millis(100),
        ).is_err() {
            // Port is free
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
