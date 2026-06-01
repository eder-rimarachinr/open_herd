/// Handlers HTTP para el dominio de Sites — delegan en los use cases de aplicación.
/// Cada handler tiene una sola responsabilidad: deserializar la request,
/// llamar al use case, y serializar la respuesta o el error.
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;

use crate::{
    application::site::create_site::CreateSiteCommand,
    application::site::update_site::UpdateSiteCommand,
    domain::errors::ApplicationError,
    infrastructure::{
        container::AppContainer,
        persistence::site_mapper,
    },
};

pub type ContainerRef = Arc<AppContainer>;

// ── Respuestas de error ────────────────────────────────────────────────────────

fn domain_err(e: ApplicationError) -> impl IntoResponse {
    let (status, msg) = match &e {
        ApplicationError::Domain(de) => {
            use crate::domain::errors::DomainError::*;
            let code = match de {
                SiteNotFound(_)        => StatusCode::NOT_FOUND,
                DomainAlreadyExists(_) => StatusCode::CONFLICT,
                InvalidTld
                | InvalidDomain(_)
                | InvalidPath(_)       => StatusCode::BAD_REQUEST,
                _                      => StatusCode::UNPROCESSABLE_ENTITY,
            };
            (code, e.to_string())
        }
        ApplicationError::Infrastructure(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        }
    };
    (status, Json(serde_json::json!({ "error": msg }))).into_response()
}

// ── POST /api/v1/sites ─────────────────────────────────────────────────────────

pub async fn create_site(
    State(container): State<ContainerRef>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let domain = match body["domain"].as_str() {
        Some(d) => d.to_string(),
        None => return (StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "domain required" }))).into_response(),
    };
    let path = match body["path"].as_str() {
        Some(p) => p.to_string(),
        None => return (StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "path required" }))).into_response(),
    };

    let default_php = container.legacy.config.read().default_php.clone();

    let cmd = CreateSiteCommand { domain, path, default_php: Some(default_php) };
    match container.create_site_uc.execute(cmd).await {
        Ok(site) => {
            let legacy = site_mapper::to_legacy(&site);
            container.legacy.log(format!("Site created: {}", legacy.domain));
            (StatusCode::CREATED, Json(legacy)).into_response()
        }
        Err(e) => domain_err(e).into_response(),
    }
}

// ── PUT /api/v1/sites/:id ─────────────────────────────────────────────────────

pub async fn update_site(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let cmd = UpdateSiteCommand {
        site_id:     id,
        php_version: body["php_version"].as_str().map(str::to_string),
        active:      body["active"].as_bool(),
    };
    match container.update_site_uc.execute(cmd).await {
        Ok(site) => Json(site_mapper::to_legacy(&site)).into_response(),
        Err(e)   => domain_err(e).into_response(),
    }
}

// ── DELETE /api/v1/sites/:id ──────────────────────────────────────────────────

pub async fn delete_site(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match container.delete_site_uc.execute(&id).await {
        Ok(()) => {
            container.legacy.log(format!("Site deleted: {}", id));
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        Err(e) => domain_err(e).into_response(),
    }
}

// ── POST /api/v1/sites/:id/ssl ────────────────────────────────────────────────

pub async fn enable_ssl(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    use crate::daemon::{models::AsyncTask, ssl as ssl_mgr};

    // Marcar como pendiente para que el frontend empiece a hacer polling
    ssl_mgr::set_task(&container.legacy.ssl_tasks, &id, "pending", "Starting SSL issuance…", None);

    let container2 = container.clone();
    let site_id    = id.clone();

    tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Handle::current();
        let set = |s: &str, m: &str, e: Option<String>| {
            ssl_mgr::set_task(&container2.legacy.ssl_tasks, &site_id, s, m, e);
        };

        set("running", "Issuing SSL certificate…", None);
        match rt.block_on(container2.enable_ssl_uc.execute(&site_id)) {
            Ok(()) => {
                container2.legacy.log(format!("SSL enabled: {}", site_id));
                set("done", "SSL certificate issued and nginx reloaded", None);
            }
            Err(e) => {
                let msg = e.to_string();
                set("error", &msg, Some(msg.clone()));
            }
        }
    });

    Json(AsyncTask {
        state:   "pending".into(),
        message: "SSL issuance started".into(),
        error:   None,
    }).into_response()
}

// ── DELETE /api/v1/sites/:id/ssl ─────────────────────────────────────────────

pub async fn disable_ssl(
    State(container): State<ContainerRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match container.disable_ssl_uc.execute(&id).await {
        Ok(()) => {
            // Limpiar el task de SSL progress si lo había
            container.legacy.ssl_tasks.lock().remove(&id);
            container.legacy.log(format!("SSL disabled: {}", id));

            // Devolver el sitio actualizado en formato legacy para compatibilidad con la GUI
            let site_opt = container.legacy.sites.read().get(&id).cloned();
            match site_opt {
                Some(s) => Json(s).into_response(),
                None    => Json(serde_json::json!({ "ok": true })).into_response(),
            }
        }
        Err(e) => domain_err(e).into_response(),
    }
}
