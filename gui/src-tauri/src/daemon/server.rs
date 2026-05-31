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
        .route("/api/v1/sites/{id}", get(get_site).put(update_site).delete(delete_site))
        .route("/api/v1/sites/{id}/ssl", post(enable_ssl).delete(disable_ssl))
        .route("/api/v1/sites/{id}/ssl/progress", get(ssl_progress))
        .route("/api/v1/sites/{id}/refresh-config", post(refresh_site_config))
        .route("/api/v1/sites/{id}/info", get(get_site_info))
        .route("/api/v1/sites/{id}/open-folder", post(open_site_folder))
        // PHP
        .route("/api/v1/php/versions", get(list_php_versions))
        .route("/api/v1/php/catalog", get(php_catalog))
        .route("/api/v1/php/detect", post(detect_php))
        .route("/api/v1/php/install", post(install_php))
        .route("/api/v1/php/install/{major}/progress", get(install_php_progress))
        .route("/api/v1/php/versions/{version}/start", post(start_php_fpm))
        .route("/api/v1/php/versions/{version}/stop", post(stop_php_fpm))
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
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect(&format!("Failed to bind to {}", addr));
    println!("Daemon API listening on http://{}", addr);
    axum::serve(listener, router).await.expect("Server error");
}
