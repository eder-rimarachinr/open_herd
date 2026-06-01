use axum::{Json, extract::{Path, State}, http::StatusCode, response::IntoResponse};
use std::sync::Arc;
use crate::{
    application::php::ini_parser,
    daemon::models::{InstallProgress, PhpExtension, PhpIniConfig, PhpSetting},
    infrastructure::container::AppContainer,
};

pub type ContainerRef = Arc<AppContainer>;

fn infra_err(e: impl std::fmt::Display) -> impl IntoResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({ "error": e.to_string() })))
}

// ── GET /api/v1/php/versions ─────────────────────────────────────────────────

pub async fn list_php_versions(State(container): State<ContainerRef>) -> impl IntoResponse {
    let versions = container.detect_php_uc.execute().await.unwrap_or_default();
    let legacy = container.legacy.php_versions.read().clone();
    let _ = versions; // detector already updated AppState.php_versions
    Json(legacy)
}

// ── GET /api/v1/php/catalog ───────────────────────────────────────────────────

pub async fn php_catalog(State(container): State<ContainerRef>) -> impl IntoResponse {
    container.detect_php_uc.execute().await.ok();
    let versions = container.legacy.php_versions.read().clone();
    let running  = crate::daemon::php::running_versions(&container.legacy.php_proc);
    Json(crate::daemon::routes::build_catalog_pub(&versions, &running))
}

// ── POST /api/v1/php/detect ───────────────────────────────────────────────────

pub async fn detect_php(State(container): State<ContainerRef>) -> impl IntoResponse {
    let installs = container.detect_php_uc.execute().await.unwrap_or_default();
    container.legacy.log(format!("PHP detect: found {} version(s)", installs.len()));
    let versions = container.legacy.php_versions.read().clone();
    let running  = crate::daemon::php::running_versions(&container.legacy.php_proc);
    Json(crate::daemon::routes::build_catalog_pub(&versions, &running))
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
    let php_dir = container.legacy.config.read().php_dir.clone();
    container.legacy.log(format!("Starting PHP {} download", major));
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
    let prog = container.legacy.downloads.php.lock().get(&major).cloned();
    match prog {
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
    let php_dir  = container.legacy.config.read().php_dir.clone();
    let ini_path = std::path::Path::new(&php_dir).join(&major).join("php.ini");
    if !ini_path.exists() {
        return (StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "php.ini not found — install this PHP version first" })))
            .into_response();
    }
    match std::fs::read_to_string(&ini_path) {
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

    match container.update_php_ini_uc.execute(cmd).await {
        Err(e) => return infra_err(e).into_response(),
        Ok(()) => {}
    }

    container.legacy.log(format!("php.ini updated for PHP {}", major));

    // Devolver config actualizada
    let updated = std::fs::read_to_string(&ini_path).unwrap_or_default();
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
    let known: Option<Vec<crate::domain::ports::process_manager::PhpInstallation>> =
        Some(container.legacy.php_versions.read().iter().map(|v| {
            crate::domain::ports::process_manager::PhpInstallation {
                major: v.major.clone(), version: v.version.clone(), binary_path: v.binary_path.clone(),
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
