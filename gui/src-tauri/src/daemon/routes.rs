use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use uuid::Uuid;

use super::models::{DaemonStatus, ServicesStatus, Site};
use super::state::{AppState, save_sites};

pub type AppStateRef = Arc<AppState>;

fn ok<T: serde::Serialize>(data: T) -> impl IntoResponse {
    (StatusCode::OK, Json(serde_json::json!({ "data": data })))
}

fn err(status: StatusCode, msg: &str) -> impl IntoResponse {
    (status, Json(serde_json::json!({ "error": msg })))
}

// GET /api/v1/status
pub async fn get_status(State(state): State<AppStateRef>) -> impl IntoResponse {
    let uptime = state.started_at.elapsed().as_secs();
    let nginx = state.nginx.read().clone();
    let php = state.php_versions.read().clone();
    ok(DaemonStatus {
        ok: true,
        version: env!("CARGO_PKG_VERSION").into(),
        uptime,
        services: ServicesStatus { nginx, php },
    })
}

// GET /api/v1/sites
pub async fn list_sites(State(state): State<AppStateRef>) -> impl IntoResponse {
    let sites = state.sites.read();
    let mut list: Vec<Site> = sites.values().cloned().collect();
    list.sort_by(|a, b| a.domain.cmp(&b.domain));
    ok(list)
}

// POST /api/v1/sites
pub async fn create_site(
    State(state): State<AppStateRef>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let domain = match body["domain"].as_str() {
        Some(d) => d.to_string(),
        None => return err(StatusCode::BAD_REQUEST, "domain required").into_response(),
    };
    let path = match body["path"].as_str() {
        Some(p) => p.to_string(),
        None => return err(StatusCode::BAD_REQUEST, "path required").into_response(),
    };

    let site = Site {
        id: Uuid::new_v4().to_string(),
        domain: domain.clone(),
        path,
        project_type: detect_project_type(&domain),
        php_version: state.config.read().default_php.clone(),
        ssl: false,
        nginx: false,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    state.sites.write().insert(site.id.clone(), site.clone());
    let _ = save_sites(&state);
    (StatusCode::CREATED, Json(serde_json::json!({ "data": site }))).into_response()
}

// GET /api/v1/sites/:id
pub async fn get_site(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.sites.read().get(&id).cloned() {
        Some(s) => ok(s).into_response(),
        None => err(StatusCode::NOT_FOUND, "site not found").into_response(),
    }
}

// DELETE /api/v1/sites/:id
pub async fn delete_site(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let removed = state.sites.write().remove(&id);
    match removed {
        Some(_) => {
            let _ = save_sites(&state);
            (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
        }
        None => err(StatusCode::NOT_FOUND, "site not found").into_response(),
    }
}

// PUT /api/v1/sites/:id
pub async fn update_site(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let mut sites = state.sites.write();
    let site = match sites.get_mut(&id) {
        Some(s) => s,
        None => return err(StatusCode::NOT_FOUND, "site not found").into_response(),
    };
    if let Some(v) = body["phpVersion"].as_str() { site.php_version = v.to_string(); }
    if let Some(v) = body["nginx"].as_bool() { site.nginx = v; }
    let updated = site.clone();
    drop(sites);
    let _ = save_sites(&state);
    ok(updated).into_response()
}

// GET /api/v1/services/status
pub async fn services_status(State(state): State<AppStateRef>) -> impl IntoResponse {
    let nginx = state.nginx.read().clone();
    let php = state.php_versions.read().clone();
    ok(ServicesStatus { nginx, php })
}

// POST /api/v1/services/start  (stub — services wired in later)
pub async fn start_services(_: State<AppStateRef>) -> impl IntoResponse {
    ok(serde_json::json!({ "started": true }))
}

// POST /api/v1/services/stop
pub async fn stop_services(_: State<AppStateRef>) -> impl IntoResponse {
    ok(serde_json::json!({ "stopped": true }))
}

// GET /api/v1/php/versions
pub async fn list_php_versions(State(state): State<AppStateRef>) -> impl IntoResponse {
    ok(state.php_versions.read().clone())
}

// GET /api/v1/nginx/status
pub async fn nginx_status(State(state): State<AppStateRef>) -> impl IntoResponse {
    ok(state.nginx.read().clone())
}

// GET /api/v1/config
pub async fn get_config(State(state): State<AppStateRef>) -> impl IntoResponse {
    ok(state.config.read().clone())
}

// POST /api/v1/daemon/quit
pub async fn quit_daemon() -> impl IntoResponse {
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        std::process::exit(0);
    });
    ok(serde_json::json!({ "ok": true }))
}

fn detect_project_type(path: &str) -> String {
    let p = std::path::Path::new(path);
    if p.join("artisan").exists() && p.join("public").exists() {
        return "laravel".into();
    }
    if p.join("spark").exists() {
        return "codeigniter4".into();
    }
    if p.join("wp-config.php").exists() || p.join("wp-login.php").exists() {
        return "wordpress".into();
    }
    if p.join("dist").join("index.html").exists() || p.join("build").join("index.html").exists() {
        return "spa".into();
    }
    if p.join("index.html").exists() {
        return "static".into();
    }
    "generic".into()
}
