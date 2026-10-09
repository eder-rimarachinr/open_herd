use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;
use std::sync::Arc;
use crate::domain::ports::task::TaskState;

use super::extract::ApiJson;

use crate::{
    application::site::{
        bulk_add_sites::BulkSiteItem,
        create_site::CreateSiteCommand,
        scan_sites::ScanSitesCommand,
        update_site::UpdateSiteCommand,
    },
    domain::{errors::ApplicationError, site::{entity::Site, value_objects::SiteId}},
    infrastructure::{container::AppContainer, persistence::site_mapper},
};

pub type ContainerRef = Arc<AppContainer>;

// ── Error helper ──────────────────────────────────────────────────────────────

fn domain_err(e: &ApplicationError) -> impl IntoResponse {
    let (status, msg) = match e {
        ApplicationError::Domain(de) => {
            use crate::domain::errors::DomainError::*;
            let code = match de {
                SiteNotFound(_)        => StatusCode::NOT_FOUND,
                DomainAlreadyExists(_) => StatusCode::CONFLICT,
                InvalidTld | InvalidDomain(_) | InvalidPath(_) => StatusCode::BAD_REQUEST,
                Persistence(_)         => StatusCode::INTERNAL_SERVER_ERROR,
                SslAlreadyActive(_) | SslCannotEnable(_) => StatusCode::UNPROCESSABLE_ENTITY,
            };
            (code, e.to_string())
        }
        ApplicationError::Infrastructure(_) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    (status, Json(serde_json::json!({ "error": msg }))).into_response()
}

fn not_found(msg: &str) -> impl IntoResponse {
    (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": msg }))).into_response()
}

/// Loads a site through the repository; the `Err` is the ready-made 404 / 500.
async fn find_site(container: &ContainerRef, id: &str) -> Result<Site, Box<axum::response::Response>> {
    match container.site_repo.find_by_id(&SiteId::from_string(id)).await {
        Ok(Some(site)) => Ok(site),
        Ok(None)       => Err(Box::new(not_found("site not found").into_response())),
        Err(e)         => Err(Box::new(domain_err(&ApplicationError::Domain(e)).into_response())),
    }
}

// ── GET /api/v1/sites ─────────────────────────────────────────────────────────

pub async fn list_sites(State(container): State<ContainerRef>) -> impl IntoResponse {
    match container.site_repo.list_all().await {
        Ok(sites) => {
            let legacy: Vec<_> = sites.iter().map(site_mapper::to_legacy).collect();
            Json(legacy).into_response()
        }
        Err(e) => domain_err(&ApplicationError::Domain(e)).into_response(),
    }
}

// ── GET /api/v1/sites/:id ─────────────────────────────────────────────────────

pub async fn get_site(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match find_site(&container, &id).await {
        Ok(site)  => Json(site_mapper::to_legacy(&site)).into_response(),
        Err(resp) => *resp,
    }
}

// ── POST /api/v1/sites ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateSiteRequest {
    domain: String,
    path:   String,
}

pub async fn create_site(
    State(container): State<ContainerRef>,
    ApiJson(body): ApiJson<CreateSiteRequest>,
) -> impl IntoResponse {
    let default_php = container.config.get().default_php;
    let cmd = CreateSiteCommand { domain: body.domain, path: body.path, default_php: Some(default_php) };
    match container.create_site_uc.execute(cmd).await {
        Ok(site) => {
            let legacy = site_mapper::to_legacy(&site);
            container.logger.log(format!("Site created: {}", legacy.domain));
            (StatusCode::CREATED, Json(legacy)).into_response()
        }
        Err(e) => domain_err(&e).into_response(),
    }
}

// ── PUT /api/v1/sites/:id ─────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct UpdateSiteRequest {
    php_version: Option<String>,
    active:      Option<bool>,
}

pub async fn update_site(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<UpdateSiteRequest>,
) -> impl IntoResponse {
    let cmd = UpdateSiteCommand { site_id: id, php_version: body.php_version, active: body.active };
    match container.update_site_uc.execute(cmd).await {
        Ok(site) => Json(site_mapper::to_legacy(&site)).into_response(),
        Err(e)   => domain_err(&e).into_response(),
    }
}

// ── DELETE /api/v1/sites/:id ──────────────────────────────────────────────────

pub async fn delete_site(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match container.delete_site_uc.execute(&id).await {
        Ok(()) => {
            container.logger.log(format!("Site deleted: {}", id));
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        Err(e) => domain_err(&e).into_response(),
    }
}

// ── POST /api/v1/sites/:id/ssl ────────────────────────────────────────────────

pub async fn enable_ssl(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    use crate::infrastructure::dto::AsyncTask;
    container.ssl_tasks.set(&id, TaskState::Pending, "Starting SSL issuance…", None);
    let container2 = container.clone();
    let site_id    = id.clone();
    // The use case is fully async (the mkcert download is async reqwest and the
    // mkcert subprocesses run on the blocking pool), so spawn it directly.
    tokio::spawn(async move {
        container2.ssl_tasks.set(&site_id, TaskState::Running, "Issuing SSL certificate…", None);
        match container2.enable_ssl_uc.execute(&site_id).await {
            Ok(()) => {
                container2.logger.log(format!("SSL enabled: {}", site_id));
                container2.ssl_tasks.set(&site_id, TaskState::Done, "SSL certificate issued and nginx reloaded", None);
            }
            Err(e) => {
                let msg = e.to_string();
                container2.ssl_tasks.set(&site_id, TaskState::Error, &msg, Some(msg.clone()));
            }
        }
    });
    Json(AsyncTask { state: TaskState::Pending, message: "SSL issuance started".into(), error: None })
        .into_response()
}

// ── DELETE /api/v1/sites/:id/ssl ─────────────────────────────────────────────

pub async fn disable_ssl(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match container.disable_ssl_uc.execute(&id).await {
        Ok(()) => {
            container.ssl_tasks.remove(&id);
            container.logger.log(format!("SSL disabled: {}", id));
            match find_site(&container, &id).await {
                Ok(site) => Json(site_mapper::to_legacy(&site)).into_response(),
                Err(_)   => Json(serde_json::json!({ "ok": true })).into_response(),
            }
        }
        Err(e) => domain_err(&e).into_response(),
    }
}

// ── GET /api/v1/sites/:id/ssl/progress ───────────────────────────────────────

pub async fn ssl_progress(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    use crate::infrastructure::dto::AsyncTask;
    if let Some(task) = container.ssl_tasks.get(&id) {
        return Json(AsyncTask { state: task.state, message: task.message, error: task.error }).into_response();
    }
    match find_site(&container, &id).await {
        Ok(site) => Json(AsyncTask {
            state:   TaskState::Done,
            message: if site.ssl.is_enabled() { "SSL active".into() } else { "SSL disabled".into() },
            error:   None,
        }).into_response(),
        Err(resp) => *resp,
    }
}

// ── POST /api/v1/sites/scan ───────────────────────────────────────────────────

pub async fn scan_sites(State(container): State<ContainerRef>) -> impl IntoResponse {
    let cfg = container.config.get();
    let (scan_dirs, default_php) = (cfg.scanned_dirs, cfg.default_php);
    if scan_dirs.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
            "error": "No scanned directories configured. Add a directory first."
        }))).into_response();
    }
    let cmd = ScanSitesCommand { dirs: scan_dirs, default_php: Some(default_php) };
    match container.scan_sites_uc.execute(cmd).await {
        Ok(result) => {
            container.logger.log(format!("Scan complete: {} new site(s) found", result.added.len()));
            if !result.not_found.is_empty() && result.added.is_empty() {
                return (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                    "error": format!("Directory not found: {}. Check the path and try again.", result.not_found.join(", "))
                }))).into_response();
            }
            match container.site_repo.list_all().await {
                Ok(sites) => Json(sites.iter().map(site_mapper::to_legacy).collect::<Vec<_>>()).into_response(),
                Err(e) => domain_err(&ApplicationError::Domain(e)).into_response(),
            }
        }
        Err(e) => domain_err(&e).into_response(),
    }
}

// ── POST /api/v1/sites/bulk ───────────────────────────────────────────────────

/// Fields default to empty so one incomplete item is skipped by the use case
/// (bulk never aborts on a bad item) instead of rejecting the whole request.
#[derive(Deserialize)]
pub struct BulkSiteRequest {
    #[serde(default)] domain: String,
    #[serde(default)] path:   String,
}

pub async fn bulk_add_sites(
    State(container): State<ContainerRef>,
    ApiJson(body): ApiJson<Vec<BulkSiteRequest>>,
) -> impl IntoResponse {
    let default_php = container.config.get().default_php;
    let items: Vec<BulkSiteItem> = body.into_iter()
        .map(|item| BulkSiteItem { domain: item.domain, path: item.path })
        .collect();
    match container.bulk_add_sites_uc.execute(items, Some(default_php)).await {
        Ok(sites) => Json(sites.iter().map(site_mapper::to_legacy).collect::<Vec<_>>()).into_response(),
        Err(e) => domain_err(&e).into_response(),
    }
}

// ── POST /api/v1/sites/:id/refresh-config ────────────────────────────────────

pub async fn refresh_site_config(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match container.refresh_site_config_uc.execute(&id).await {
        Ok(site) => {
            container.logger.log(format!("Config refreshed: {} (type: {})", site.domain, site.project_type));
            Json(site_mapper::to_legacy(&site)).into_response()
        }
        Err(e) => domain_err(&e).into_response(),
    }
}

// ── GET /api/v1/sites/:id/info ────────────────────────────────────────────────

pub async fn get_site_info(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let site = match find_site(&container, &id).await {
        Ok(site)  => site_mapper::to_legacy(&site),
        Err(resp) => return *resp,
    };
    // Reads several files from the project: keep it off the async workers.
    match tokio::task::spawn_blocking(move || crate::infrastructure::site_info::read(&site)).await {
        Ok(info) => Json(info).into_response(),
        Err(e)   => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() }))).into_response(),
    }
}

// ── POST /api/v1/sites/:id/open-folder ───────────────────────────────────────

pub async fn open_site_folder(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let site = match find_site(&container, &id).await {
        Ok(site)  => site,
        Err(resp) => return *resp,
    };
    match container.file_manager.open_folder(site.path.as_path()).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => domain_err(&ApplicationError::Infrastructure(e)).into_response(),
    }
}
