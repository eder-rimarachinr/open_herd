use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use std::sync::Arc;
use crate::{
    infrastructure::{
        container::AppContainer,
        dto::{AsyncTask, NginxInfo, PhpVersionStatus, ServiceStatus},
        nginx::process as nginx_mgr,
        php::process as php,
    },
};

pub type ContainerRef = Arc<AppContainer>;

fn infra_err(e: impl std::fmt::Display) -> impl IntoResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() })))
}

// ── GET /api/v1/nginx/status ──────────────────────────────────────────────────

pub async fn nginx_status(State(container): State<ContainerRef>) -> impl IntoResponse {
    use crate::infrastructure::dto::NginxStatus;
    let status = container.web_server.status().await;
    Json(NginxStatus { running: status.running, version: status.version, pid: status.pid })
}

// ── GET /api/v1/nginx/info ────────────────────────────────────────────────────

pub async fn nginx_info(State(container): State<ContainerRef>) -> impl IntoResponse {
    let nginx_dir = container.legacy.config.read().nginx_dir.clone();
    let binary    = nginx_mgr::find_nginx_binary(&nginx_dir);
    let running   = container.web_server.status().await.running;
    let version     = binary.as_ref().and_then(|b| nginx_mgr::get_nginx_version(b)).unwrap_or_default();
    let binary_path = binary.map(|b| b.to_string_lossy().to_string()).unwrap_or_default();
    Json(NginxInfo {
        installed: !binary_path.is_empty(), running, version, binary_path,
        config_valid: None, config_error: String::new(), error_log: String::new(),
        downloadable: cfg!(target_os = "windows"), os: std::env::consts::OS.into(),
    })
}

// ── POST /api/v1/nginx/download ───────────────────────────────────────────────

pub async fn download_nginx(State(container): State<ContainerRef>) -> impl IntoResponse {
    let nginx_dir = container.legacy.config.read().nginx_dir.clone();
    container.legacy.log(format!("Starting nginx download to {}", nginx_dir));
    match container.download_nginx_uc.execute(&nginx_dir).await {
        Ok(state) => Json(AsyncTask { state, message: "Download started".into(), error: None }).into_response(),
        Err(e)    => infra_err(e).into_response(),
    }
}

// ── GET /api/v1/nginx/download/progress ──────────────────────────────────────

pub async fn nginx_download_progress(State(container): State<ContainerRef>) -> impl IntoResponse {
    let prog = container.legacy.downloads.nginx.lock().clone();
    match prog {
        Some(p) => Json(AsyncTask { state: p.state, message: p.message, error: p.error }),
        None    => Json(AsyncTask { state: "done".into(), message: String::new(), error: None }),
    }
}

// ── POST /api/v1/nginx/start ──────────────────────────────────────────────────

pub async fn start_nginx(State(container): State<ContainerRef>) -> impl IntoResponse {
    match container.start_nginx_uc.execute().await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => { container.legacy.log(format!("Nginx start failed: {}", e)); infra_err(e).into_response() }
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

// ── GET /api/v1/services/status ───────────────────────────────────────────────

pub async fn services_status(State(container): State<ContainerRef>) -> impl IntoResponse {
    Json(build_service_status(&container).await)
}

// ── POST /api/v1/services/start ───────────────────────────────────────────────

pub async fn start_services(State(container): State<ContainerRef>) -> impl IntoResponse {
    use crate::application::services::start_services::StartServicesCommand;
    let default_php = container.legacy.config.read().default_php.clone();
    let cmd = StartServicesCommand { default_php };
    let container2 = container.clone();
    tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(container2.start_services_uc.execute(cmd))
    }).await.ok();
    Json(build_service_status(&container).await)
}

// ── POST /api/v1/services/stop ────────────────────────────────────────────────

pub async fn stop_services(State(container): State<ContainerRef>) -> impl IntoResponse {
    let container2 = container.clone();
    tokio::task::spawn_blocking(move || {
        tokio::runtime::Handle::current().block_on(container2.stop_services_uc.execute())
    }).await.ok();
    Json(build_service_status(&container).await)
}

// ── Helper ────────────────────────────────────────────────────────────────────

async fn build_service_status(container: &ContainerRef) -> ServiceStatus {
    let nginx_running = container.web_server.status().await.running;
    let running = php::running_versions(&container.legacy.php_proc);
    let versions = container.php_version_repo.list().await;
    let php_status: Vec<PhpVersionStatus> = running.iter().map(|major| {
        let ver = versions.iter()
            .find(|v| &v.major == major).map(|v| v.version.clone())
            .unwrap_or_else(|| major.clone());
        PhpVersionStatus { major: major.clone(), version: ver, running: true }
    }).collect();
    let all = nginx_running && !php_status.is_empty() && php_status.iter().all(|p| p.running);
    ServiceStatus { nginx: nginx_running, php_versions: php_status, all_running: all }
}
