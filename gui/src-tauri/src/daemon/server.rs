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
        .route("/api/v1/status", get(get_status))
        .route("/api/v1/sites", get(list_sites).post(create_site))
        .route("/api/v1/sites/{id}", get(get_site).put(update_site).delete(delete_site))
        .route("/api/v1/services/status", get(services_status))
        .route("/api/v1/services/start", post(start_services))
        .route("/api/v1/services/stop", post(stop_services))
        .route("/api/v1/php/versions", get(list_php_versions))
        .route("/api/v1/nginx/status", get(nginx_status))
        .route("/api/v1/config", get(get_config))
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
