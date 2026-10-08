pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod ports;

use infrastructure::{
    config::{resolve_base_dir, Config},
    container::AppContainer,
    state::AppState,
};
use ports::http::{config_handlers, server};

// The remaining `expect`s are startup failures with no recovery path (no logs
// dir, no runtime, no window icon); aborting with a clear message is the intent.
#[allow(clippy::expect_used)]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let base_dir = resolve_base_dir();

    // Create base + logs dirs early so any subsequent panic can be captured.
    let log_dir = base_dir.join("logs");
    std::fs::create_dir_all(&log_dir)
        .expect("Cannot create base/logs dir");

    let infrastructure::fs::Loaded { value: config, warning: config_warning } = Config::load(&base_dir);
    let api_addr = config.api_addr.clone();

    try_shutdown_previous(&api_addr);

    let state     = AppState::new(base_dir.clone(), config);
    let container = AppContainer::new(state);
    if let Some(w) = config_warning { container.logger.log(w); }

    let container_for_api  = container.clone();
    let container_for_tray = container.clone();
    let crash_log = log_dir.join("daemon-crash.log");
    let crash_log_thread = crash_log.clone();

    std::thread::Builder::new()
        .name("open-herd-daemon".into())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                tokio::runtime::Runtime::new()
                    .expect("Failed to create Tokio runtime")
                    .block_on(server::start(container_for_api));
            }));
            if let Err(payload) = result {
                let msg = payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                    .unwrap_or_else(|| "Unknown panic in daemon thread".into());
                eprintln!("[open-herd] daemon crash: {}", msg);
                let _ = std::fs::write(&crash_log_thread, format!("{}\n", msg));
            }
        })
        .expect("Failed to spawn daemon thread");

    let api_addr_setup = api_addr;
    let log_dir_setup  = log_dir;
    let crash_log_setup = crash_log;

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .on_window_event(|window, event| {
            // Closing the window minimizes to the tray instead of quitting — the
            // daemon (nginx/PHP) keeps running. Real shutdown only happens via the
            // sidebar Quit button or the tray menu's "Salir" item, both of which
            // hit /api/v1/daemon/quit (or graceful_shutdown directly, for the tray).
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(move |app| {
            use tauri::Manager;
            #[cfg(debug_assertions)]
            if let Some(w) = app.get_webview_window("main") { w.open_devtools(); }

            // -- System tray -------------------------------------------------------
            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

            let show_item = MenuItem::with_id(app, "show", "Mostrar Open Herd", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("no default window icon set in tauri.conf.json").clone())
                .tooltip("Open Herd")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        let container = container_for_tray.clone();
                        std::thread::spawn(move || {
                            tokio::runtime::Runtime::new()
                                .expect("tokio rt for shutdown")
                                .block_on(config_handlers::graceful_shutdown(&container));
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Verify the daemon came up; show a native error dialog if it did not.
            let handle     = app.handle().clone();
            let addr       = api_addr_setup;
            let crash_path = crash_log_setup;
            let base       = log_dir_setup.parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default();

            std::thread::spawn(move || {
                let socket_addr: std::net::SocketAddr = addr.parse().unwrap_or(server::DEFAULT_API_ADDR);

                // Wait up to 10 s (20 × 500 ms) for the daemon to bind.
                let up = (0..20).any(|_| {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    std::net::TcpStream::connect_timeout(
                        &socket_addr,
                        std::time::Duration::from_millis(200),
                    ).is_ok()
                });

                if !up {
                    let detail = std::fs::read_to_string(&crash_path)
                        .unwrap_or_else(|_| "No crash log available.".into());
                    let msg = format!(
                        "The Open Herd daemon could not start on {addr}.\n\n\
                         Reason: {detail}\n\n\
                         Full log: {base}/logs/daemon-crash.log\n\n\
                         Common causes:\n\
                         • Port {port} is already in use by another process\n\
                         • A previous instance did not quit cleanly\n\n\
                         Try restarting the app. If the problem persists, \
                         free port {port} or edit base_dir/config.json to change api_addr.",
                        addr   = addr,
                        detail = detail.trim(),
                        base   = base,
                        port   = socket_addr.port(),
                    );
                    use tauri_plugin_dialog::DialogExt;
                    handle.dialog()
                        .message(msg)
                        .title("Open Herd — Daemon Failed to Start")
                        .blocking_show();
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn try_shutdown_previous(api_addr: &str) {
    #[cfg(target_os = "windows")]
    use std::os::windows::process::CommandExt;
    #[cfg(target_os = "windows")]
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let url = format!("http://{}/api/v1/daemon/quit", api_addr);
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new("curl");
    cmd.args(["-s", "-X", "POST", "--max-time", "1", &url]);
    #[cfg(target_os = "windows")] cmd.creation_flags(CREATE_NO_WINDOW);
    let _ = cmd.output();
    let socket_addr: std::net::SocketAddr = api_addr.parse().unwrap_or(server::DEFAULT_API_ADDR);
    if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
        &socket_addr,
        std::time::Duration::from_millis(300),
    ) {
        use std::io::Write;
        let req = format!("POST /api/v1/daemon/quit HTTP/1.0\r\nHost: {}\r\nContent-Length: 0\r\n\r\n", api_addr);
        let _ = stream.write_all(req.as_bytes());
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    while std::time::Instant::now() < deadline {
        if std::net::TcpStream::connect_timeout(&socket_addr, std::time::Duration::from_millis(100)).is_err() { break; }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
