use axum::{Router, routing::{get, post}, http::{HeaderValue, Method}};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use super::routes;
use crate::infrastructure::container::AppContainer;
use crate::ports::http::site_handlers;

pub fn build_router(container: Arc<AppContainer>) -> Router {
    // Restrict CORS to the origins that legitimately call the daemon:
    //   - tauri://localhost          → Tauri webview on Windows
    //   - https://tauri.localhost    → Tauri webview (some configs)
    //   - http://localhost:1420      → Vite dev server
    //   - http://127.0.0.1:1420     → Vite dev server (numeric)
    let allowed: Vec<HeaderValue> = [
        "tauri://localhost",
        "https://tauri.localhost",
        "http://localhost:1420",
        "http://127.0.0.1:1420",
    ]
    .iter()
    .filter_map(|o| o.parse().ok())
    .collect();

    let cors = CorsLayer::new()
        .allow_origin(allowed)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(tower_http::cors::Any);

    Router::new()
        // Status
        .route("/api/v1/status", get(routes::get_status))
        // Sites — mutaciones usan los nuevos handlers; lecturas usan el legacy
        .route("/api/v1/sites",
            get(routes::list_sites)
            .post(site_handlers::create_site))
        .route("/api/v1/sites/scan",  post(site_handlers::scan_sites))
        .route("/api/v1/sites/bulk",  post(site_handlers::bulk_add_sites))
        .route("/api/v1/sites/:id",
            get(routes::get_site)
            .put(site_handlers::update_site)
            .delete(site_handlers::delete_site))
        .route("/api/v1/sites/:id/ssl",
            post(site_handlers::enable_ssl)
            .delete(site_handlers::disable_ssl))
        .route("/api/v1/sites/:id/ssl/progress",    get(routes::ssl_progress))
        .route("/api/v1/sites/:id/refresh-config",  post(site_handlers::refresh_site_config))
        .route("/api/v1/sites/:id/info",            get(routes::get_site_info))
        .route("/api/v1/sites/:id/open-folder",     post(routes::open_site_folder))
        // PHP
        .route("/api/v1/php/versions",              get(routes::list_php_versions))
        .route("/api/v1/php/catalog",               get(routes::php_catalog))
        .route("/api/v1/php/detect",                post(routes::detect_php))
        .route("/api/v1/php/install",               post(routes::install_php))
        .route("/api/v1/php/install/:major/progress", get(routes::install_php_progress))
        .route("/api/v1/php/versions/:version/start", post(routes::start_php_fpm))
        .route("/api/v1/php/versions/:version/stop",  post(routes::stop_php_fpm))
        .route("/api/v1/php/versions/:version/ini",
            get(routes::get_php_ini).put(routes::update_php_ini))
        // Nginx
        .route("/api/v1/nginx/status",              get(routes::nginx_status))
        .route("/api/v1/nginx/info",                get(routes::nginx_info))
        .route("/api/v1/nginx/download",            post(routes::download_nginx))
        .route("/api/v1/nginx/download/progress",   get(routes::nginx_download_progress))
        .route("/api/v1/nginx/start",               post(routes::start_nginx))
        .route("/api/v1/nginx/stop",                post(routes::stop_nginx))
        .route("/api/v1/nginx/reload",              post(routes::reload_nginx))
        // Services
        .route("/api/v1/services/status",           get(routes::services_status))
        .route("/api/v1/services/start",            post(routes::start_services))
        .route("/api/v1/services/stop",             post(routes::stop_services))
        // Config
        .route("/api/v1/config",
            get(routes::get_config).put(routes::update_config))
        // Daemon
        .route("/api/v1/daemon/logs",               get(routes::daemon_logs))
        .route("/api/v1/daemon/quit",               post(routes::quit_daemon))
        .layer(cors)
        .with_state(container)
}

pub async fn start(container: Arc<AppContainer>) {
    let addr = container.legacy.config.read().api_addr.clone();
    let router = build_router(container);

    let listener = bind_with_retry(&addr).await;

    println!("Daemon API listening on http://{}", addr);
    axum::serve(listener, router).await.expect("Server error");
}

/// Bind the TCP listener with SO_REUSEADDR and automatic retry.
///
/// Strategy (matches what Herd / Laravel Valet do):
///   1. Set SO_REUSEADDR so the OS allows re-using a port in TIME_WAIT state.
///   2. Retry up to 30 times × 300 ms = 9 s total — enough to survive a hot-
///      reload cycle where the previous process takes a moment to fully exit.
///   3. Panic only after all retries are exhausted, with a clear error message.
async fn bind_with_retry(addr: &str) -> tokio::net::TcpListener {
    let socket_addr: std::net::SocketAddr = addr
        .parse()
        .unwrap_or_else(|_| "127.0.0.1:7878".parse().unwrap());

    let mut last_err = String::new();

    for attempt in 0..30 {
        // Build a raw socket so we can set SO_REUSEADDR before binding
        let socket = if socket_addr.is_ipv4() {
            tokio::net::TcpSocket::new_v4()
        } else {
            tokio::net::TcpSocket::new_v6()
        };

        match socket {
            Ok(sock) => {
                // SO_REUSEADDR lets us bind even if the port is in TIME_WAIT
                let _ = sock.set_reuseaddr(true);
                match sock.bind(socket_addr) {
                    Ok(()) => {
                        match sock.listen(1024) {
                            Ok(listener) => return listener,
                            Err(e) => last_err = e.to_string(),
                        }
                    }
                    Err(e) => last_err = e.to_string(),
                }
            }
            Err(e) => last_err = e.to_string(),
        }

        if attempt < 9 {
            // Only log the first 10 attempts to avoid log spam
            eprintln!("Port {} busy (attempt {}), retrying…", addr, attempt + 1);
        } else if attempt == 9 {
            eprintln!("Port {} still busy after 10 attempts, continuing silently…", addr);
        }

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }

    panic!(
        "Cannot bind {} after 30 attempts: {}\n\
         \nTroubleshooting:\
         \n  • Kill any running Open Herd process: taskkill /F /IM phpenv-gui.exe\
         \n  • Check what is using the port: netstat -ano | findstr :7878",
        addr, last_err
    )
}
