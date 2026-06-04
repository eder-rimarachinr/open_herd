use axum::{Json, extract::State, response::IntoResponse};
use std::sync::Arc;
use crate::infrastructure::{container::AppContainer, dto::DaemonStatus};

pub type ContainerRef = Arc<AppContainer>;

// ── GET /api/v1/status ────────────────────────────────────────────────────────

pub async fn get_status(State(container): State<ContainerRef>) -> impl IntoResponse {
    use crate::infrastructure::{dto::NginxRunning, php::process as php_mgr};
    let uptime   = container.legacy.started_at.elapsed().as_secs();
    let nginx    = container.legacy.nginx.read();
    let php_vers = php_mgr::running_versions(&container.legacy.php_proc);
    Json(DaemonStatus {
        status:       "ok".into(),
        version:      env!("CARGO_PKG_VERSION").into(),
        uptime:       format!("{}s", uptime),
        os:           std::env::consts::OS.into(),
        php_versions: php_vers,
        nginx:        NginxRunning { running: nginx.running },
    })
}

// ── GET /api/v1/config ────────────────────────────────────────────────────────

pub async fn get_config(State(container): State<ContainerRef>) -> impl IntoResponse {
    Json(container.legacy.config.read().clone())
}

// ── PUT /api/v1/config ────────────────────────────────────────────────────────

pub async fn update_config(
    State(container): State<ContainerRef>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let mut cfg = container.legacy.config.write();
    if let Some(v) = body["default_php"].as_str()    { cfg.default_php  = v.to_string(); }
    if let Some(v) = body["http_port"].as_u64()      { cfg.http_port    = v as u16; }
    if let Some(v) = body["https_port"].as_u64()     { cfg.https_port   = v as u16; }
    if let Some(arr) = body["scanned_dirs"].as_array() {
        cfg.scanned_dirs = arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
    }
    if let Some(arr) = body["custom_php_dirs"].as_array() {
        cfg.custom_php_dirs = arr.iter().filter_map(|v| v.as_str().map(str::to_string)).collect();
    }
    let updated = cfg.clone();
    drop(cfg);
    let _ = updated.save(&container.legacy.base_dir);
    Json(updated)
}

// ── GET /api/v1/daemon/logs ───────────────────────────────────────────────────

pub async fn daemon_logs(State(container): State<ContainerRef>) -> impl IntoResponse {
    let log  = container.legacy.daemon_log.read();
    let last: Vec<&String> = log.iter().rev().take(100).collect::<Vec<_>>().into_iter().rev().collect();
    Json(serde_json::json!({ "logs": last.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n") }))
}

// ── POST /api/v1/daemon/quit ──────────────────────────────────────────────────

pub async fn quit_daemon(State(container): State<ContainerRef>) -> impl IntoResponse {
    tokio::spawn(async move {
        graceful_shutdown(&container).await;
    });
    Json(serde_json::json!({ "ok": true }))
}

/// Detiene nginx y PHP y luego termina el proceso.
/// Llamado tanto desde el endpoint /quit como desde el evento CloseRequested.
pub async fn graceful_shutdown(container: &AppContainer) {
    use crate::infrastructure::{nginx::process as nginx_mgr, php::process as php_mgr};
    php_mgr::stop_all(&container.legacy, &container.legacy.php_proc);
    let _ = nginx_mgr::stop(&container.legacy, &container.legacy.nginx_proc);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    std::process::exit(0);
}
