use axum::{Router, routing::{get, post}, http::Method};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

use crate::infrastructure::container::AppContainer;
use crate::ports::http::{config_handlers, nginx_handlers, php_handlers, site_handlers};

pub fn build_router(container: Arc<AppContainer>) -> Router {
    // The daemon only binds 127.0.0.1 (loopback), so allowing any origin is safe.
    // A strict origin list causes silent CORS failures in production Tauri builds
    // because the WebView2 Origin header format can vary between dev and release.
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers(tower_http::cors::Any);

    Router::new()
        .route("/api/v1/status",                        get(config_handlers::get_status))
        .route("/api/v1/sites",                         get(site_handlers::list_sites).post(site_handlers::create_site))
        .route("/api/v1/sites/scan",                    post(site_handlers::scan_sites))
        .route("/api/v1/sites/bulk",                    post(site_handlers::bulk_add_sites))
        .route("/api/v1/sites/:id",                     get(site_handlers::get_site).put(site_handlers::update_site).delete(site_handlers::delete_site))
        .route("/api/v1/sites/:id/ssl",                 post(site_handlers::enable_ssl).delete(site_handlers::disable_ssl))
        .route("/api/v1/sites/:id/ssl/progress",        get(site_handlers::ssl_progress))
        .route("/api/v1/sites/:id/refresh-config",      post(site_handlers::refresh_site_config))
        .route("/api/v1/sites/:id/info",                get(site_handlers::get_site_info))
        .route("/api/v1/sites/:id/open-folder",         post(site_handlers::open_site_folder))
        .route("/api/v1/php/versions",                  get(php_handlers::list_php_versions))
        .route("/api/v1/php/catalog",                   get(php_handlers::php_catalog))
        .route("/api/v1/php/detect",                    post(php_handlers::detect_php))
        .route("/api/v1/php/install",                   post(php_handlers::install_php))
        .route("/api/v1/php/install/:major/progress",   get(php_handlers::install_php_progress))
        .route("/api/v1/php/versions/:version/start",   post(php_handlers::start_php_fpm))
        .route("/api/v1/php/versions/:version/stop",    post(php_handlers::stop_php_fpm))
        .route("/api/v1/php/versions/:version/ini",     get(php_handlers::get_php_ini).put(php_handlers::update_php_ini))
        .route("/api/v1/nginx/status",                  get(nginx_handlers::nginx_status))
        .route("/api/v1/nginx/info",                    get(nginx_handlers::nginx_info))
        .route("/api/v1/nginx/download",                post(nginx_handlers::download_nginx))
        .route("/api/v1/nginx/download/progress",       get(nginx_handlers::nginx_download_progress))
        .route("/api/v1/nginx/start",                   post(nginx_handlers::start_nginx))
        .route("/api/v1/nginx/stop",                    post(nginx_handlers::stop_nginx))
        .route("/api/v1/nginx/reload",                  post(nginx_handlers::reload_nginx))
        .route("/api/v1/services/status",               get(nginx_handlers::services_status))
        .route("/api/v1/services/start",                post(nginx_handlers::start_services))
        .route("/api/v1/services/stop",                 post(nginx_handlers::stop_services))
        .route("/api/v1/config",                        get(config_handlers::get_config).put(config_handlers::update_config))
        .route("/api/v1/daemon/logs",                   get(config_handlers::daemon_logs))
        .route("/api/v1/daemon/quit",                   post(config_handlers::quit_daemon))
        .layer(cors)
        .with_state(container)
}

pub async fn start(container: Arc<AppContainer>) {
    let addr   = container.legacy.config.read().api_addr.clone();
    let router = build_router(container);
    let listener = bind_with_retry(&addr).await;
    println!("Daemon API listening on http://{}", addr);
    axum::serve(listener, router).await.expect("Server error");
}

async fn bind_with_retry(addr: &str) -> tokio::net::TcpListener {
    let socket_addr: std::net::SocketAddr = addr.parse().unwrap_or_else(|_| "127.0.0.1:7878".parse().unwrap());
    let mut last_err = String::new();
    for attempt in 0..30 {
        let socket = if socket_addr.is_ipv4() { tokio::net::TcpSocket::new_v4() } else { tokio::net::TcpSocket::new_v6() };
        match socket {
            Ok(sock) => {
                let _ = sock.set_reuseaddr(true);
                match sock.bind(socket_addr) {
                    Ok(()) => match sock.listen(1024) {
                        Ok(l) => return l,
                        Err(e) => last_err = e.to_string(),
                    },
                    Err(e) => last_err = e.to_string(),
                }
            }
            Err(e) => last_err = e.to_string(),
        }
        if attempt < 9 {
            eprintln!("[open-herd] port {} busy (attempt {}), retrying…", addr, attempt + 1);
        } else if attempt == 9 {
            eprintln!("[open-herd] port {} still busy after 10 attempts; hint: run `netstat -ano | findstr :{}` to find the process holding it.", addr, socket_addr.port());
        }
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    // Surface the port number and current OS error so the crash log is actionable.
    panic!(
        "Cannot bind {} after 30 attempts: {}. \
         If another process owns this port, free it or change api_addr in config.json.",
        addr, last_err
    )
}
