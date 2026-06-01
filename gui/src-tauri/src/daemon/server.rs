use axum::{Router, routing::{get, post}};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};

use super::routes::*;
use super::state::AppState;

pub fn build_router(state: Arc<AppState>) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Status
        .route("/api/v1/status", get(get_status))
        // Sites
        .route("/api/v1/sites", get(list_sites).post(create_site))
        .route("/api/v1/sites/scan", post(scan_sites))
        .route("/api/v1/sites/bulk", post(bulk_add_sites))
        .route("/api/v1/sites/:id", get(get_site).put(update_site).delete(delete_site))
        .route("/api/v1/sites/:id/ssl", post(enable_ssl).delete(disable_ssl))
        .route("/api/v1/sites/:id/ssl/progress", get(ssl_progress))
        .route("/api/v1/sites/:id/refresh-config", post(refresh_site_config))
        .route("/api/v1/sites/:id/info", get(get_site_info))
        .route("/api/v1/sites/:id/open-folder", post(open_site_folder))
        // PHP
        .route("/api/v1/php/versions", get(list_php_versions))
        .route("/api/v1/php/catalog", get(php_catalog))
        .route("/api/v1/php/detect", post(detect_php))
        .route("/api/v1/php/install", post(install_php))
        .route("/api/v1/php/install/:major/progress", get(install_php_progress))
        .route("/api/v1/php/versions/:version/start", post(start_php_fpm))
        .route("/api/v1/php/versions/:version/stop", post(stop_php_fpm))
        .route("/api/v1/php/versions/:version/ini", get(get_php_ini).put(update_php_ini))
        // Nginx
        .route("/api/v1/nginx/status", get(nginx_status))
        .route("/api/v1/nginx/info", get(nginx_info))
        .route("/api/v1/nginx/download", post(download_nginx))
        .route("/api/v1/nginx/download/progress", get(nginx_download_progress))
        .route("/api/v1/nginx/start", post(start_nginx))
        .route("/api/v1/nginx/stop", post(stop_nginx))
        .route("/api/v1/nginx/reload", post(reload_nginx))
        // Services
        .route("/api/v1/services/status", get(services_status))
        .route("/api/v1/services/start", post(start_services))
        .route("/api/v1/services/stop", post(stop_services))
        // Config
        .route("/api/v1/config", get(get_config).put(update_config))
        // Daemon
        .route("/api/v1/daemon/logs", get(daemon_logs))
        .route("/api/v1/daemon/quit", post(quit_daemon))
        .layer(cors)
        .with_state(state)
}

pub async fn start(state: Arc<AppState>) {
    let addr = state.config.read().api_addr.clone();
    let router = build_router(state);

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
