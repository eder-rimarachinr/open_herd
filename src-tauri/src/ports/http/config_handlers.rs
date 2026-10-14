use axum::{Json, extract::State, http::StatusCode, response::{IntoResponse, Response}};
use serde::Deserialize;
use std::sync::Arc;
use crate::infrastructure::{config::Config, container::AppContainer, dto::DaemonStatus};
use super::extract::ApiJson;

pub type ContainerRef = Arc<AppContainer>;

// ── GET /api/v1/status ────────────────────────────────────────────────────────

pub async fn get_status(State(container): State<ContainerRef>) -> impl IntoResponse {
    use crate::infrastructure::dto::NginxRunning;
    let uptime   = container.started_at.elapsed().as_secs();
    let nginx    = container.web_server.status().await;
    let php_vers = container.php_process_port.running_majors().await;
    Json(DaemonStatus {
        status:       "ok".into(),
        version:      env!("CARGO_PKG_VERSION").into(),
        uptime:       format!("{}s", uptime),
        os:           std::env::consts::OS.into(),
        php_versions: php_vers,
        nginx:        NginxRunning { running: nginx.running },
        warnings:     container.load_warnings.clone(),
    })
}

// ── GET /api/v1/config ────────────────────────────────────────────────────────

pub async fn get_config(State(container): State<ContainerRef>) -> impl IntoResponse {
    Json(container.config.get())
}

// ── PUT /api/v1/config ────────────────────────────────────────────────────────

/// Validates the patch on a copy, writes it to disk, and only then makes it the
/// live config — an invalid body or a failed save leaves both untouched.
pub async fn update_config(
    State(container): State<ContainerRef>,
    ApiJson(patch): ApiJson<ConfigPatch>,
) -> Response {
    let previous = container.config.get();
    let mut updated = previous.clone();
    if let Err(msg) = patch.apply(&mut updated) {
        return error(StatusCode::BAD_REQUEST, msg);
    }
    if let Err(e) = container.config.replace(updated.clone()).await {
        return error(StatusCode::INTERNAL_SERVER_ERROR, format!("could not save config.json: {e}"));
    }
    if let Err(response) = apply_listen_ports(&container, &previous, &updated).await {
        return response;
    }
    Json(updated).into_response()
}

/// Ports are the only settings nginx reads at start (everything else is read
/// live on each use), so a port change restarts a running nginx. If it can't
/// listen on the new ports (another web server holds them), the previous
/// config is restored and nginx is brought back on the old ports — the user
/// keeps working sites and gets a 409 explaining why nothing changed.
async fn apply_listen_ports(container: &AppContainer, previous: &Config, updated: &Config) -> Result<(), Response> {
    let ports = |c: &Config| (c.http_port, c.https_port);
    if ports(previous) == ports(updated) || !container.web_server.is_running().await {
        return Ok(());
    }
    let Err(e) = container.restart_nginx_uc.execute().await else {
        container.logger.log(format!("nginx restarted on ports {}/{}", updated.http_port, updated.https_port));
        return Ok(());
    };

    let reason = format!(
        "nginx could not start on ports {}/{} ({e}). The previous ports {}/{} were restored.",
        updated.http_port, updated.https_port, previous.http_port, previous.https_port,
    );
    tracing::warn!("{reason}");
    if let Err(save) = container.config.replace(previous.clone()).await {
        return Err(error(StatusCode::INTERNAL_SERVER_ERROR, format!("{reason} But restoring config.json failed: {save}")));
    }
    if let Err(restart) = container.restart_nginx_uc.execute().await {
        return Err(error(StatusCode::INTERNAL_SERVER_ERROR, format!("{reason} But nginx also failed to start on them: {restart}")));
    }
    Err(error(StatusCode::CONFLICT, reason))
}

fn error(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(serde_json::json!({ "error": msg.into() }))).into_response()
}

/// Body of `PUT /config`: only the fields present are changed. Wrong JSON
/// types are rejected by deserialization (400); `apply` checks value ranges.
/// Ports are read as `u64` so an out-of-range number gets a clear message
/// instead of a generic overflow error.
#[derive(Deserialize)]
pub struct ConfigPatch {
    default_php:     Option<String>,
    http_port:       Option<u64>,
    https_port:      Option<u64>,
    scanned_dirs:    Option<Vec<String>>,
    custom_php_dirs: Option<Vec<String>>,
}

impl ConfigPatch {
    fn apply(self, cfg: &mut Config) -> Result<(), String> {
        if let Some(v) = self.default_php {
            if !super::php_handlers::valid_major(&v) {
                return Err(format!("default_php must look like \"8.2\", got {v:?}"));
            }
            cfg.default_php = v;
        }
        if let Some(v) = self.http_port  { cfg.http_port  = port("http_port", v)?; }
        if let Some(v) = self.https_port { cfg.https_port = port("https_port", v)?; }
        if cfg.http_port == cfg.https_port {
            return Err(format!("http_port and https_port must differ (both {})", cfg.http_port));
        }
        if let Some(v) = self.scanned_dirs    { cfg.scanned_dirs    = v; }
        if let Some(v) = self.custom_php_dirs { cfg.custom_php_dirs = v; }
        Ok(())
    }
}

fn port(field: &str, n: u64) -> Result<u16, String> {
    u16::try_from(n).ok()
        .filter(|&p| p != 0)
        .ok_or_else(|| format!("{field} must be an integer between 1 and 65535, got {n}"))
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
