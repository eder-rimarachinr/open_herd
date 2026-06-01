use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use std::sync::Arc;
use crate::{daemon::php, infrastructure::container::AppContainer};

pub type ContainerRef = Arc<AppContainer>;

fn infra_err(e: impl std::fmt::Display) -> impl IntoResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() })))
}

// ── POST /api/v1/nginx/start ──────────────────────────────────────────────────

pub async fn start_nginx(State(container): State<ContainerRef>) -> impl IntoResponse {
    match container.start_nginx_uc.execute().await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => {
            container.legacy.log(format!("Nginx start failed: {}", e));
            infra_err(e).into_response()
        }
    }
}

// ── POST /api/v1/nginx/stop ───────────────────────────────────────────────────

pub async fn stop_nginx(State(container): State<ContainerRef>) -> impl IntoResponse {
    match container.stop_nginx_uc.execute().await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => infra_err(e).into_response(),
    }
}

// ── POST /api/v1/nginx/reload ─────────────────────────────────────────────────

pub async fn reload_nginx(State(container): State<ContainerRef>) -> impl IntoResponse {
    match container.reload_nginx_uc.execute().await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => infra_err(e).into_response(),
    }
}

// ── POST /api/v1/services/start ───────────────────────────────────────────────

pub async fn start_services(State(container): State<ContainerRef>) -> impl IntoResponse {
    use crate::application::services::start_services::StartServicesCommand;
    let default_php = container.legacy.config.read().default_php.clone();
    let cmd = StartServicesCommand { default_php };

    let container2 = container.clone();
    tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current()
            .block_on(container2.start_services_uc.execute(cmd))
    }).await.ok();

    build_service_status(&container)
}

// ── POST /api/v1/services/stop ────────────────────────────────────────────────

pub async fn stop_services(State(container): State<ContainerRef>) -> impl IntoResponse {
    let container2 = container.clone();
    tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current()
            .block_on(container2.stop_services_uc.execute())
    }).await.ok();

    build_service_status(&container)
}

// ── Helper ────────────────────────────────────────────────────────────────────

fn build_service_status(container: &ContainerRef) -> impl IntoResponse {
    use crate::daemon::{models::{PhpVersionStatus, ServiceStatus}, nginx as nginx_mgr};

    let nginx_running = nginx_mgr::is_running(&container.legacy.nginx_proc);
    if !nginx_running {
        container.legacy.nginx.write().running = false;
    }

    let running = php::running_versions(&container.legacy.php_proc);
    let php_status: Vec<PhpVersionStatus> = running.iter().map(|major| {
        let ver = container.legacy.php_versions.read().iter()
            .find(|v| &v.major == major)
            .map(|v| v.version.clone())
            .unwrap_or_else(|| major.clone());
        PhpVersionStatus { major: major.clone(), version: ver, running: true }
    }).collect();

    let all = nginx_running && !php_status.is_empty() && php_status.iter().all(|p| p.running);
    Json(ServiceStatus { nginx: nginx_running, php_versions: php_status, all_running: all })
}
