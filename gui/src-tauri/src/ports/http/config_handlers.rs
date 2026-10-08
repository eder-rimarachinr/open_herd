use axum::{Json, extract::State, http::StatusCode, response::{IntoResponse, Response}};
use std::sync::Arc;
use crate::infrastructure::{config::Config, container::AppContainer, dto::DaemonStatus};

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

/// Validates the patch on a copy, writes it to disk, and only then makes it the
/// live config — an invalid body or a failed save leaves both untouched.
pub async fn update_config(
    State(container): State<ContainerRef>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let mut updated = container.legacy.config.read().clone();
    if let Err(msg) = apply_config_patch(&mut updated, &body) {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg }))).into_response();
    }
    let to_save  = updated.clone();
    let base_dir = container.legacy.base_dir.clone();
    let saved = tokio::task::spawn_blocking(move || to_save.save(&base_dir)).await
        .map_err(anyhow::Error::from)
        .and_then(|r| r);
    if let Err(e) = saved {
        return (StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("could not save config.json: {e}") }))).into_response();
    }
    *container.legacy.config.write() = updated.clone();
    Json(updated).into_response()
}

/// Applies the fields present in `body`. Unknown fields are ignored; a present
/// field with a wrong type or value is an error rather than being dropped.
fn apply_config_patch(cfg: &mut Config, body: &serde_json::Value) -> Result<(), String> {
    if let Some(v) = body.get("default_php") {
        let v = v.as_str().ok_or("default_php must be a string")?;
        if !super::php_handlers::valid_major(v) {
            return Err(format!("default_php must look like \"8.2\", got {v:?}"));
        }
        cfg.default_php = v.to_string();
    }
    if let Some(v) = body.get("http_port")  { cfg.http_port  = parse_port("http_port", v)?; }
    if let Some(v) = body.get("https_port") { cfg.https_port = parse_port("https_port", v)?; }
    if cfg.http_port == cfg.https_port {
        return Err(format!("http_port and https_port must differ (both {})", cfg.http_port));
    }
    if let Some(v) = body.get("scanned_dirs")    { cfg.scanned_dirs    = parse_string_list("scanned_dirs", v)?; }
    if let Some(v) = body.get("custom_php_dirs") { cfg.custom_php_dirs = parse_string_list("custom_php_dirs", v)?; }
    Ok(())
}

fn parse_port(field: &str, v: &serde_json::Value) -> Result<u16, String> {
    v.as_u64()
        .and_then(|n| u16::try_from(n).ok())
        .filter(|&n| n != 0)
        .ok_or_else(|| format!("{field} must be an integer between 1 and 65535, got {v}"))
}

fn parse_string_list(field: &str, v: &serde_json::Value) -> Result<Vec<String>, String> {
    v.as_array()
        .ok_or_else(|| format!("{field} must be an array of strings"))?
        .iter()
        .map(|item| item.as_str().map(str::to_string).ok_or_else(|| format!("{field} must contain only strings")))
        .collect()
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
