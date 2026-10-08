use axum::{Json, extract::{Path, State}, http::StatusCode, response::IntoResponse};
use std::sync::Arc;
use crate::{
    application::php::ini_parser,
    infrastructure::{container::AppContainer, dto::{InstallProgress, PhpExtension, PhpIniConfig, PhpSetting}},
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
fn valid_major(s: &str) -> bool {
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
    let running  = crate::infrastructure::php::process::running_versions(&container.legacy.php_proc);
    Json(crate::infrastructure::php::catalog::build_catalog(&versions, &running))
}

// ── POST /api/v1/php/detect ───────────────────────────────────────────────────

pub async fn detect_php(State(container): State<ContainerRef>) -> impl IntoResponse {
    let installs = container.detect_php_uc.execute().await.unwrap_or_default();
    container.logger.log(format!("PHP detect: found {} version(s)", installs.len()));
    let versions: Vec<_> = installs.iter().map(crate::infrastructure::php::version_mapper::to_legacy).collect();
    let running  = crate::infrastructure::php::process::running_versions(&container.legacy.php_proc);
    Json(crate::infrastructure::php::catalog::build_catalog(&versions, &running))
}

// ── POST /api/v1/php/install ──────────────────────────────────────────────────

pub async fn install_php(
    State(container): State<ContainerRef>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let major = match body["major"].as_str() {
        Some(m) => m.to_string(),
        None => return (StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "major required" }))).into_response(),
    };
    if !valid_major(&major) { return bad_request("invalid PHP version"); }
    let php_dir = container.legacy.config.read().php_dir.clone();
    container.logger.log(format!("Starting PHP {} download", major));
    match container.install_php_uc.execute(&major, &php_dir).await {
        Ok(state) => Json(serde_json::json!({ "ok": true, "state": state })).into_response(),
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
            let error = if p.state == "error" { Some(p.message.clone()) } else { p.error.clone() };
            Json(InstallProgress { major, state: p.state, message: p.message, percent: p.percent, error })
                .into_response()
        }
        None => {
            let php_dir   = container.legacy.config.read().php_dir.clone();
            let installed = std::path::Path::new(&php_dir).join(&major).join("php-cgi.exe").exists()
                || std::path::Path::new(&php_dir).join(&major).join("php.exe").exists();
            Json(InstallProgress {
                major,
                state: if installed { "done".into() } else { "idle".into() },
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
) -> impl IntoResponse {
    if !valid_major(&major) { return bad_request("invalid PHP version"); }
    let php_dir  = container.legacy.config.read().php_dir.clone();
    let ini_path = std::path::Path::new(&php_dir).join(&major).join("php.ini");
    if !ini_path.exists() {
        return (StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "php.ini not found — install this PHP version first" })))
            .into_response();
    }
    match tokio::fs::read_to_string(&ini_path).await {
        Ok(content) => Json(PhpIniConfig {
            major,
            ini_path: ini_path.to_string_lossy().to_string(),
            extensions: ini_parser::parse_extensions(&content),
            settings:   ini_parser::parse_settings(&content),
        }).into_response(),
        Err(e) => infra_err(e).into_response(),
    }
}

// ── PUT /api/v1/php/versions/:major/ini ──────────────────────────────────────

pub async fn update_php_ini(
    State(container): State<ContainerRef>,
    Path(major): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    use crate::application::php::update_php_ini::UpdatePhpIniCommand;

    if !valid_major(&major) { return bad_request("invalid PHP version"); }
    let php_dir = container.legacy.config.read().php_dir.clone();
    let ini_path = std::path::Path::new(&php_dir).join(&major).join("php.ini");
    if !ini_path.exists() {
        return (StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "php.ini not found" }))).into_response();
    }

    let extension_changes: Vec<PhpExtension> = body["extensions"].as_array()
        .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
        .unwrap_or_default();
    let setting_changes: Vec<PhpSetting> = body["settings"].as_array()
        .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
        .unwrap_or_default();

    let cmd = UpdatePhpIniCommand { major: major.clone(), php_dir, extension_changes, setting_changes };

    if let Err(e) = container.update_php_ini_uc.execute(cmd).await { return infra_err(e).into_response() }

    container.logger.log(format!("php.ini updated for PHP {}", major));

    // Devolver config actualizada
    let updated = tokio::fs::read_to_string(&ini_path).await.unwrap_or_default();
    Json(PhpIniConfig {
        major,
        ini_path: ini_path.to_string_lossy().to_string(),
        extensions: ini_parser::parse_extensions(&updated),
        settings:   ini_parser::parse_settings(&updated),
    }).into_response()
}

// ── POST /api/v1/php/versions/:version/start ─────────────────────────────────

pub async fn start_php_fpm(
    State(container): State<ContainerRef>,
    Path(version): Path<String>,
) -> impl IntoResponse {
    if !valid_major(&version) { return bad_request("invalid PHP version"); }
    let known = Some(container.php_version_repo.list().await);
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
    if !valid_major(&version) { return bad_request("invalid PHP version"); }
    let _ = container.stop_php_uc.execute(&version).await;
    Json(serde_json::json!({ "ok": true })).into_response()
}
