use axum::{Json, extract::{Path, State}, http::StatusCode, response::IntoResponse};
use std::sync::Arc;
use crate::infrastructure::container::AppContainer;

pub type ContainerRef = Arc<AppContainer>;

fn infra_err(e: impl std::fmt::Display) -> impl IntoResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() })))
}

// ── POST /api/v1/php/detect ───────────────────────────────────────────────────

pub async fn detect_php(State(container): State<ContainerRef>) -> impl IntoResponse {
    let installs = container.detect_php_uc.execute().await.unwrap_or_default();
    container.legacy.log(format!("PHP detect: found {} version(s)", installs.len()));

    // Leer el catálogo desde el estado legacy (ya actualizado por el detector)
    let versions  = container.legacy.php_versions.read().clone();
    let running   = crate::daemon::php::running_versions(&container.legacy.php_proc);
    Json(crate::daemon::routes::build_catalog_pub(&versions, &running))
}

// ── POST /api/v1/php/versions/:version/start ─────────────────────────────────

pub async fn start_php_fpm(
    State(container): State<ContainerRef>,
    Path(version): Path<String>,
) -> impl IntoResponse {
    let known: Option<Vec<crate::domain::ports::process_manager::PhpInstallation>> =
        Some(container.legacy.php_versions.read().iter().map(|v| {
            crate::domain::ports::process_manager::PhpInstallation {
                major:       v.major.clone(),
                version:     v.version.clone(),
                binary_path: v.binary_path.clone(),
            }
        }).collect());

    match container.start_php_uc.execute(&version, known).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => infra_err(e).into_response(),
    }
}

// ── POST /api/v1/php/versions/:version/stop ──────────────────────────────────

pub async fn stop_php_fpm(
    State(container): State<ContainerRef>,
    Path(version): Path<String>,
) -> impl IntoResponse {
    let _ = container.stop_php_uc.execute(&version).await;
    Json(serde_json::json!({ "ok": true }))
}
