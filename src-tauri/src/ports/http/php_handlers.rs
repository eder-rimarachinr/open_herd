use axum::{Json, extract::{Path, State}, http::StatusCode, response::{IntoResponse, Response}};
use serde::Deserialize;
use std::sync::Arc;
use crate::domain::ports::task::TaskState;

use super::{error::app_error, extract::ApiJson};
use crate::{
    application::php::{ini_parser::{ExtensionChange, SettingChange}, update_php_ini::UpdatePhpIniCommand},
    infrastructure::{container::AppContainer, dto::{InstallProgress, PhpIniConfig}},
};

pub type ContainerRef = Arc<AppContainer>;

fn infra_err(e: impl std::fmt::Display) -> impl IntoResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() })))
}

fn bad_request(msg: &str) -> axum::response::Response {
    (StatusCode::BAD_REQUEST, Json(serde_json::json!({ "error": msg }))).into_response()
}

/// A PHP `major` arrives from a URL path segment and is concatenated into a
/// filesystem path (`php_dir/<major>/php.ini`). Restrict it to `N.N` so it can
/// never contain `/`, `..` or other traversal sequences.
pub(crate) fn valid_major(s: &str) -> bool {
    let mut parts = s.split('.');
    let major = parts.next();
    let minor = parts.next();
    parts.next().is_none()
        && major.is_some_and(|m| !m.is_empty() && m.len() <= 2 && m.bytes().all(|b| b.is_ascii_digit()))
        && minor.is_some_and(|m| !m.is_empty() && m.len() <= 2 && m.bytes().all(|b| b.is_ascii_digit()))
}

// ── GET /api/v1/php/versions ─────────────────────────────────────────────────

pub async fn list_php_versions(State(container): State<ContainerRef>) -> impl IntoResponse {
    let versions = container.detect_php_uc.execute().await.unwrap_or_default();
    let legacy: Vec<_> = versions.iter().map(crate::infrastructure::php::version_mapper::to_legacy).collect();
    Json(legacy)
}

// ── GET /api/v1/php/catalog ───────────────────────────────────────────────────

pub async fn php_catalog(State(container): State<ContainerRef>) -> impl IntoResponse {
    container.detect_php_uc.execute().await.ok();
    let versions: Vec<_> = container.php_version_repo.list().await.iter()
        .map(crate::infrastructure::php::version_mapper::to_legacy).collect();
    let running  = container.php_process_port.running_majors().await;
    Json(crate::infrastructure::php::catalog::build_catalog(&versions, &running))
}

// ── POST /api/v1/php/detect ───────────────────────────────────────────────────

pub async fn detect_php(State(container): State<ContainerRef>) -> impl IntoResponse {
    let installs = container.detect_php_uc.execute().await.unwrap_or_default();
    container.logger.log(format!("PHP detect: found {} version(s)", installs.len()));
    let versions: Vec<_> = installs.iter().map(crate::infrastructure::php::version_mapper::to_legacy).collect();
    let running  = container.php_process_port.running_majors().await;
    Json(crate::infrastructure::php::catalog::build_catalog(&versions, &running))
}

// ── POST /api/v1/php/install ──────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct InstallPhpRequest {
    major: String,
}

pub async fn install_php(
    State(container): State<ContainerRef>,
    ApiJson(InstallPhpRequest { major }): ApiJson<InstallPhpRequest>,
) -> impl IntoResponse {
    if !valid_major(&major) { return bad_request("invalid PHP version"); }
    let php_dir = container.config.get().php_dir;
    container.logger.log(format!("Starting PHP {} download", major));
    match container.install_php_uc.execute(&major, &php_dir).await {
        Ok(state) => Json(serde_json::json!({ "ok": true, "state": state.as_str() })).into_response(),
        Err(e)    => infra_err(e).into_response(),
    }
}

// ── GET /api/v1/php/install/:major/progress ───────────────────────────────────

pub async fn install_php_progress(
    State(container): State<ContainerRef>,
    Path(major): Path<String>,
) -> impl IntoResponse {
    if !valid_major(&major) { return bad_request("invalid PHP version"); }
    match container.downloads.php_progress(&major).await {
        Some(p) => {
            let error = if p.state == TaskState::Error { Some(p.message.clone()) } else { p.error.clone() };
            Json(InstallProgress { major, state: p.state, message: p.message, percent: p.percent, error })
                .into_response()
        }
        None => {
            let php_dir   = container.config.get().php_dir;
            let dir       = std::path::Path::new(&php_dir).join(&major);
            let installed = crate::infrastructure::download::is_install_complete(&dir)
                && (dir.join("php-cgi.exe").exists() || dir.join("php.exe").exists());
            Json(InstallProgress {
                major,
                state: if installed { TaskState::Done } else { TaskState::Idle },
                message: String::new(),
                percent: if installed { 100 } else { 0 },
                error: None,
            }).into_response()
        }
    }
}

// ── GET /api/v1/php/versions/:major/ini ──────────────────────────────────────

pub async fn get_php_ini(
    State(container): State<ContainerRef>,
    Path(major): Path<String>,
) -> Response {
    if !valid_major(&major) { return bad_request("invalid PHP version"); }
    match container.get_php_ini_uc.execute(&major).await {
        Ok(view) => Json(PhpIniConfig::from(view)).into_response(),
        Err(e)   => app_error(&e),
    }
}

// ── PUT /api/v1/php/versions/:major/ini ──────────────────────────────────────

/// Only what each change needs; extra fields the GUI sends (category, label,
/// hint) are ignored. A malformed entry rejects the request (400) instead of
/// being dropped silently.
#[derive(Deserialize)]
pub struct ExtensionToggle { name: String, enabled: bool }

#[derive(Deserialize)]
pub struct SettingUpdate { key: String, value: String }

#[derive(Deserialize)]
pub struct UpdatePhpIniRequest {
    #[serde(default)] extensions: Vec<ExtensionToggle>,
    #[serde(default)] settings:   Vec<SettingUpdate>,
}

pub async fn update_php_ini(
    State(container): State<ContainerRef>,
    Path(major): Path<String>,
    ApiJson(body): ApiJson<UpdatePhpIniRequest>,
) -> Response {
    if !valid_major(&major) { return bad_request("invalid PHP version"); }
    let cmd = UpdatePhpIniCommand {
        major: major.clone(),
        extension_changes: body.extensions.into_iter().map(|e| ExtensionChange { name: e.name, enabled: e.enabled }).collect(),
        setting_changes:   body.settings.into_iter().map(|s| SettingChange { key: s.key, value: s.value }).collect(),
    };
    match container.update_php_ini_uc.execute(cmd).await {
        Ok(view) => {
            container.logger.log(format!("php.ini updated for PHP {major}"));
            Json(PhpIniConfig::from(view)).into_response()
        }
        Err(e) => app_error(&e),
    }
}

// ── POST /api/v1/php/versions/:version/start ─────────────────────────────────

pub async fn start_php_fpm(
    State(container): State<ContainerRef>,
    Path(version): Path<String>,
) -> impl IntoResponse {
    if !valid_major(&version) { return bad_request("invalid PHP version"); }
    match container.start_php_uc.execute(&version).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => infra_err(e).into_response(),
    }
}

// ── POST /api/v1/php/versions/:version/stop ──────────────────────────────────

pub async fn stop_php_fpm(
    State(container): State<ContainerRef>,
    Path(version): Path<String>,
) -> impl IntoResponse {
    if !valid_major(&version) { return bad_request("invalid PHP version"); }
    let _ = container.stop_php_uc.execute(&version).await;
    Json(serde_json::json!({ "ok": true })).into_response()
}
