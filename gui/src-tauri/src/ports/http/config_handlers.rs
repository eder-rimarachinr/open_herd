use axum::{Json, extract::State, response::IntoResponse};
use std::sync::Arc;
use crate::infrastructure::{container::AppContainer, dto::DaemonStatus};

pub type ContainerRef = Arc<AppContainer>;

// ── GET /api/v1/status ────────────────────────────────────────────────────────

pub async fn get_status(State(container): State<ContainerRef>) -> impl IntoResponse {
    use crate::infrastructure::{dto::NginxRunning, php::process as php_mgr};
    let uptime   = container.legacy.started_at.elapsed().as_secs();
    let nginx    = container.web_server.status().await;
    let php_vers = php_mgr::running_versions(&container.legacy.php_proc);
    Json(DaemonStatus {
        status:       "ok".into(),
        version:      env!("CARGO_PKG_VERSION").into(),
        uptime:       format!("{}s", uptime),
        os:           std::env::consts::OS.into(),
        php_versions: php_vers,
        nginx:        NginxRunning { running: nginx.running },
        warnings:     container.legacy.load_warnings.clone(),
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
    let updated = {
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
        cfg.clone()
    };
    let to_save  = updated.clone();
    let base_dir = container.legacy.base_dir.clone();
    // TODO(fase 3): report save failures to the caller instead of ignoring them.
    let _ = tokio::task::spawn_blocking(move || to_save.save(&base_dir)).await;
    Json(updated)
}

// ── GET /api/v1/daemon/logs ───────────────────────────────────────────────────

pub async fn daemon_logs(State(container): State<ContainerRef>) -> impl IntoResponse {
    let last = container.logger.recent(100);
    Json(serde_json::json!({ "logs": last.join("\n") }))
}

// ── POST /api/v1/daemon/quit ──────────────────────────────────────────────────

pub async fn quit_daemon(State(container): State<ContainerRef>) -> impl IntoResponse {
    tokio::spawn(async move {
        graceful_shutdown(&container).await;
    });
    Json(serde_json::json!({ "ok": true }))
}

/// Detiene nginx y PHP y luego cierra la app (vía Tauri, para que limpie el
/// icono de bandeja y la WebView). Llamado desde el endpoint /quit y desde la
/// opción "Salir" del menú de bandeja.
pub async fn graceful_shutdown(container: &AppContainer) {
    if let Err(e) = container.stop_services_uc.execute().await {
        container.logger.log(format!("Shutdown: stopping services failed: {}", e));
    }
    container.exit_app();
}
