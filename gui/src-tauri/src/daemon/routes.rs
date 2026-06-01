use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use uuid::Uuid;

use super::dns;
use super::download as dl;
use super::models::*;
use super::nginx as nginx_mgr;
use super::php as php_mgr;
use super::site_config;
use super::site_info;
use super::state::{AppState, save_sites};
use super::validate;

pub type AppStateRef = Arc<AppState>;

fn err(status: StatusCode, msg: &str) -> impl IntoResponse {
    (status, Json(serde_json::json!({ "error": msg })))
}

/// Fire-and-forget nginx reload on a blocking thread so async handlers don't stall.
fn reload_nginx_if_running(state: &AppStateRef) {
    if nginx_mgr::is_running(&state.nginx_proc) {
        let s = state.clone();
        tokio::task::spawn_blocking(move || { let _ = nginx_mgr::reload(&s); });
    }
}

// ── Status ────────────────────────────────────────────────────────────────────

pub async fn get_status(State(state): State<AppStateRef>) -> impl IntoResponse {
    let uptime = state.started_at.elapsed().as_secs();
    let nginx = state.nginx.read();
    Json(DaemonStatus {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        uptime: format!("{}s", uptime),
        os: std::env::consts::OS.into(),
        php_versions: vec![],
        nginx: NginxRunning { running: nginx.running },
    })
}

// ── Sites ─────────────────────────────────────────────────────────────────────

pub async fn list_sites(State(state): State<AppStateRef>) -> impl IntoResponse {
    let sites = state.sites.read();
    let mut list: Vec<Site> = sites.values().cloned().collect();
    list.sort_by(|a, b| a.domain.cmp(&b.domain));
    Json(list)
}

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

    // Validate inputs before touching nginx config or /etc/hosts
    if let Err(e) = validate::domain(&domain) {
        return err(StatusCode::BAD_REQUEST, &format!("Invalid domain: {}", e)).into_response();
    }
    if let Err(e) = validate::site_path(&path) {
        return err(StatusCode::BAD_REQUEST, &format!("Invalid path: {}", e)).into_response();
    }

    let now = chrono::Utc::now().to_rfc3339();
    let project_type = detect_project_type(&path);

    // Single config read — prevents inconsistency if config is updated concurrently
    let (php_version, nginx_dir, http_port) = {
        let cfg = state.config.read();
        (cfg.default_php.clone(), cfg.nginx_dir.clone(), cfg.http_port)
    };

    let site = Site {
        id: Uuid::new_v4().to_string(),
        name: domain.clone(),
        domain,
        path,
        project_type,
        php_version,
        ssl_enabled: false,
        active: false,
        created_at: now.clone(),
        updated_at: now,
    };
    state.sites.write().insert(site.id.clone(), site.clone());
    let _ = save_sites(&state);

    // Generate nginx config + DNS entry
    site_config::ensure_fastcgi_params(&nginx_dir);
    if let Err(e) = site_config::generate(&site, &nginx_dir, http_port) {
        state.log(format!("nginx config error for {}: {}", site.domain, e));
    }
    if let Err(e) = dns::add_entry(&site.domain) {
        state.log(format!("hosts entry error for {}: {}", site.domain, e));
    }
    reload_nginx_if_running(&state);

    state.log(format!("Site created: {}", site.domain));
    (StatusCode::CREATED, Json(site)).into_response()
}

pub async fn get_site(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.sites.read().get(&id).cloned() {
        Some(s) => Json(s).into_response(),
        None => err(StatusCode::NOT_FOUND, "site not found").into_response(),
    }
}

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
    if let Some(v) = body["php_version"].as_str() { site.php_version = v.to_string(); }
    if let Some(v) = body["active"].as_bool() { site.active = v; }
    if let Some(v) = body["ssl_enabled"].as_bool() { site.ssl_enabled = v; }
    site.updated_at = chrono::Utc::now().to_rfc3339();
    let updated = site.clone();
    drop(sites);
    let _ = save_sites(&state);

    // Regenerate nginx config (PHP version or active flag may have changed)
    let (nginx_dir, http_port) = {
        let cfg = state.config.read();
        (cfg.nginx_dir.clone(), cfg.http_port)
    };
    site_config::ensure_fastcgi_params(&nginx_dir);
    if let Err(e) = site_config::generate(&updated, &nginx_dir, http_port) {
        state.log(format!("nginx config error for {}: {}", updated.domain, e));
    }
    reload_nginx_if_running(&state);

    Json(updated).into_response()
}

pub async fn delete_site(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let removed = state.sites.write().remove(&id);
    match removed {
        Some(s) => {
            let _ = save_sites(&state);
            let nginx_dir = state.config.read().nginx_dir.clone();
            site_config::remove(&s, &nginx_dir);
            if let Err(e) = dns::remove_entry(&s.domain) {
                state.log(format!("hosts remove error for {}: {}", s.domain, e));
            }
            reload_nginx_if_running(&state);
            state.log(format!("Site deleted: {}", s.domain));
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        None => err(StatusCode::NOT_FOUND, "site not found").into_response(),
    }
}

pub async fn scan_sites(State(state): State<AppStateRef>) -> impl IntoResponse {
    let scan_dirs = state.config.read().scanned_dirs.clone();

    if scan_dirs.is_empty() {
        return err(StatusCode::BAD_REQUEST, "No scanned directories configured. Add a directory first.").into_response();
    }

    // Read config once outside the loop — prevents inconsistency across iterations
    let (php_version_default, nginx_dir, http_port) = {
        let cfg = state.config.read();
        (cfg.default_php.clone(), cfg.nginx_dir.clone(), cfg.http_port)
    };

    let mut added = 0u32;
    let mut not_found: Vec<String> = vec![];

    for dir in &scan_dirs {
        let p = std::path::Path::new(dir);
        if !p.exists() {
            not_found.push(dir.clone());
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(p) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() { continue; }
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                // Skip hidden dirs
                if name.starts_with('.') { continue; }
                let domain = format!("{}.test", name);
                let already = state.sites.read().values().any(|s| s.domain == domain);
                if already { continue; }
                let now = chrono::Utc::now().to_rfc3339();
                let path_str = path.to_string_lossy().to_string();
                let project_type = detect_project_type(&path_str);
                let php_version = php_version_default.clone();
                let site = Site {
                    id: Uuid::new_v4().to_string(),
                    name: domain.clone(),
                    domain: domain.clone(),
                    path: path_str,
                    project_type,
                    php_version,
                    ssl_enabled: false,
                    active: false,
                    created_at: now.clone(),
                    updated_at: now,
                };
                // Generate nginx config and DNS entry for each new site
                site_config::ensure_fastcgi_params(&nginx_dir);
                let _ = site_config::generate(&site, &nginx_dir, http_port);
                if let Err(e) = dns::add_entry(&site.domain) {
                    state.log(format!("hosts error for {}: {}", site.domain, e));
                }
                state.sites.write().insert(site.id.clone(), site.clone());
                added += 1;
            }
        }
    }

    let _ = save_sites(&state);
    reload_nginx_if_running(&state);

    if !not_found.is_empty() {
        state.log(format!("Scan: directories not found: {}", not_found.join(", ")));
        if added == 0 {
            return err(
                StatusCode::BAD_REQUEST,
                &format!("Directory not found: {}. Check the path and try again.", not_found.join(", "))
            ).into_response();
        }
    }

    state.log(format!("Scan complete: {} new site(s) found", added));

    let sites = state.sites.read();
    let mut list: Vec<Site> = sites.values().cloned().collect();
    list.sort_by(|a, b| a.domain.cmp(&b.domain));
    Json(list).into_response()
}

pub async fn bulk_add_sites(
    State(state): State<AppStateRef>,
    Json(body): Json<Vec<serde_json::Value>>,
) -> impl IntoResponse {
    // Read config once for the whole batch
    let (php_version_default, nginx_dir, http_port) = {
        let cfg = state.config.read();
        (cfg.default_php.clone(), cfg.nginx_dir.clone(), cfg.http_port)
    };
    site_config::ensure_fastcgi_params(&nginx_dir);

    for item in body {
        let domain = item["domain"].as_str().unwrap_or("").to_string();
        let path   = item["path"].as_str().unwrap_or("").to_string();
        if domain.is_empty() || path.is_empty() { continue; }

        // Skip invalid inputs silently (bulk callers may pass mixed data)
        if validate::domain(&domain).is_err() || validate::site_path(&path).is_err() { continue; }

        let already = state.sites.read().values().any(|s| s.domain == domain);
        if already { continue; }

        let now = chrono::Utc::now().to_rfc3339();
        let project_type = detect_project_type(&path);
        let site = Site {
            id: Uuid::new_v4().to_string(),
            name: domain.clone(),
            domain,
            path,
            project_type,
            php_version: php_version_default.clone(),
            ssl_enabled: false,
            active: false,
            created_at: now.clone(),
            updated_at: now,
        };

        // Generate nginx config and DNS entry — same as create_site and scan_sites
        let _ = site_config::generate(&site, &nginx_dir, http_port);
        if let Err(e) = dns::add_entry(&site.domain) {
            state.log(format!("hosts entry error for {}: {}", site.domain, e));
        }
        state.sites.write().insert(site.id.clone(), site);
    }

    let _ = save_sites(&state);
    reload_nginx_if_running(&state);

    let sites = state.sites.read();
    let mut list: Vec<Site> = sites.values().cloned().collect();
    list.sort_by(|a, b| a.domain.cmp(&b.domain));
    Json(list)
}

// SSL stubs — full mkcert integration comes later
pub async fn enable_ssl(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mut sites = state.sites.write();
    if let Some(site) = sites.get_mut(&id) {
        site.ssl_enabled = true;
        site.updated_at = chrono::Utc::now().to_rfc3339();
        let updated = site.clone();
        drop(sites);
        let _ = save_sites(&state);
        state.log(format!("SSL enabled: {}", updated.domain));
        return Json(AsyncTask { state: "done".into(), message: "SSL enabled".into(), error: None }).into_response();
    }
    err(StatusCode::NOT_FOUND, "site not found").into_response()
}

pub async fn disable_ssl(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mut sites = state.sites.write();
    if let Some(site) = sites.get_mut(&id) {
        site.ssl_enabled = false;
        site.updated_at = chrono::Utc::now().to_rfc3339();
        let updated = site.clone();
        drop(sites);
        let _ = save_sites(&state);
        return Json(updated).into_response();
    }
    err(StatusCode::NOT_FOUND, "site not found").into_response()
}

pub async fn ssl_progress(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.sites.read().get(&id) {
        Some(s) => Json(AsyncTask {
            state: "done".into(),
            message: if s.ssl_enabled { "SSL active".into() } else { "SSL disabled".into() },
            error: None,
        }).into_response(),
        None => err(StatusCode::NOT_FOUND, "site not found").into_response(),
    }
}

pub async fn refresh_site_config(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Re-detect project type and update the stored site
    let updated = {
        let mut sites = state.sites.write();
        let site = match sites.get_mut(&id) {
            Some(s) => s,
            None => return err(StatusCode::NOT_FOUND, "site not found").into_response(),
        };
        // Re-detect project type from disk
        site.project_type = detect_project_type(&site.path);
        site.updated_at = chrono::Utc::now().to_rfc3339();
        site.clone()
    };
    let _ = save_sites(&state);

    let (nginx_dir, http_port) = {
        let cfg = state.config.read();
        (cfg.nginx_dir.clone(), cfg.http_port)
    };
    if let Err(e) = site_config::generate(&updated, &nginx_dir, http_port) {
        return err(StatusCode::INTERNAL_SERVER_ERROR, &e).into_response();
    }
    if let Err(e) = dns::add_entry(&updated.domain) {
        state.log(format!("hosts entry error: {}", e));
    }
    reload_nginx_if_running(&state);
    state.log(format!("Config refreshed: {} (type: {})", updated.domain, updated.project_type));
    Json(updated).into_response()
}

pub async fn get_site_info(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.sites.read().get(&id).cloned() {
        Some(site) => Json(site_info::read(&site)).into_response(),
        None => err(StatusCode::NOT_FOUND, "site not found").into_response(),
    }
}

pub async fn open_site_folder(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let path = state.sites.read().get(&id).map(|s| s.path.clone());
    match path {
        Some(p) => {
            #[cfg(target_os = "windows")]
            let _ = std::process::Command::new("explorer").arg(&p).spawn();
            #[cfg(target_os = "linux")]
            let _ = std::process::Command::new("xdg-open").arg(&p).spawn();
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        None => err(StatusCode::NOT_FOUND, "site not found").into_response(),
    }
}

// ── PHP ───────────────────────────────────────────────────────────────────────

pub async fn list_php_versions(State(state): State<AppStateRef>) -> impl IntoResponse {
    // Always run detection — versions can change (new PHP installed, etc.)
    // Use blocking thread since it spawns child processes
    let state2 = state.clone();
    let versions = tokio::task::spawn_blocking(move || {
        let v = detect_php_versions();
        *state2.php_versions.write() = v.clone();
        v
    }).await.unwrap_or_default();
    Json(versions)
}

pub async fn php_catalog(State(state): State<AppStateRef>) -> impl IntoResponse {
    let state2 = state.clone();
    let versions = tokio::task::spawn_blocking(move || {
        let v = detect_php_versions();
        *state2.php_versions.write() = v.clone();
        v
    }).await.unwrap_or_default();
    Json(build_catalog(&versions))
}

pub async fn detect_php(State(state): State<AppStateRef>) -> impl IntoResponse {
    let state2 = state.clone();
    let versions = tokio::task::spawn_blocking(move || {
        let v = detect_php_versions();
        *state2.php_versions.write() = v.clone();
        v
    }).await.unwrap_or_default();
    state.log(format!("PHP detect: found {} version(s)", versions.len()));
    Json(build_catalog(&versions))
}

pub async fn install_php(
    State(state): State<AppStateRef>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let major = match body["major"].as_str() {
        Some(m) => m.to_string(),
        None => return err(StatusCode::BAD_REQUEST, "major required").into_response(),
    };

    // Check if already downloading
    if let Some(prog) = state.downloads.php.lock().get(&major) {
        if prog.state == "downloading" || prog.state == "extracting" {
            return Json(serde_json::json!({ "ok": true, "state": prog.state })).into_response();
        }
    }

    let php_dir = std::path::PathBuf::from(state.config.read().php_dir.clone());
    state.log(format!("Starting PHP {} download", major));
    dl::download_php(&major, &php_dir, state.downloads.clone());

    Json(serde_json::json!({ "ok": true, "state": "pending" })).into_response()
}

pub async fn install_php_progress(
    State(state): State<AppStateRef>,
    Path(major): Path<String>,
) -> impl IntoResponse {
    let prog = state.downloads.php.lock().get(&major).cloned();
    match prog {
        Some(p) => {
            let error = if p.state == "error" { Some(p.message.clone()) } else { p.error.clone() };
            Json(InstallProgress {
                major,
                state: p.state,
                message: p.message,
                percent: p.percent,
                error,
            }).into_response()
        }
        None => {
            // Check if already installed on disk
            let php_dir = state.config.read().php_dir.clone();
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

pub async fn start_php_fpm(
    State(state): State<AppStateRef>,
    Path(version): Path<String>,
) -> impl IntoResponse {
    let versions = state.php_versions.read().clone();
    let v = match versions.iter().find(|v| v.major == version || v.version == version) {
        Some(v) => v.clone(),
        // 422 not 404: the route exists; the version just isn't installed/detected yet.
        None => return err(StatusCode::UNPROCESSABLE_ENTITY, "PHP version not detected — run detect first").into_response(),
    };
    let php_proc = state.php_proc.clone();
    match php_mgr::start(&state, &php_proc, &v) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e).into_response(),
    }
}

pub async fn stop_php_fpm(
    State(state): State<AppStateRef>,
    Path(version): Path<String>,
) -> impl IntoResponse {
    let php_proc = state.php_proc.clone();
    let _ = php_mgr::stop(&state, &php_proc, &version);
    Json(serde_json::json!({ "ok": true })).into_response()
}

// ── Nginx ─────────────────────────────────────────────────────────────────────

pub async fn nginx_status(State(state): State<AppStateRef>) -> impl IntoResponse {
    Json(state.nginx.read().clone())
}

pub async fn nginx_info(State(state): State<AppStateRef>) -> impl IntoResponse {
    let config = state.config.read();
    let binary = nginx_mgr::find_nginx_binary(&config.nginx_dir);
    let installed = binary.is_some();
    let running = nginx_mgr::is_running(&state.nginx_proc);

    // Sync running state
    if !running {
        state.nginx.write().running = false;
    }

    let version = binary.as_ref()
        .and_then(|b| nginx_mgr::get_nginx_version(b))
        .unwrap_or_default();
    let binary_path = binary.map(|b| b.to_string_lossy().to_string()).unwrap_or_default();

    Json(NginxInfo {
        installed,
        running,
        version,
        binary_path,
        config_valid: None,
        config_error: String::new(),
        error_log: String::new(),
        downloadable: cfg!(target_os = "windows"),
        os: std::env::consts::OS.into(),
    })
}

pub async fn download_nginx(State(state): State<AppStateRef>) -> impl IntoResponse {
    // Don't start a second download if one is in progress
    {
        let current = state.downloads.nginx.lock();
        if let Some(ref p) = *current {
            if p.state == "downloading" || p.state == "extracting" {
                return Json(AsyncTask {
                    state: p.state.clone(),
                    message: p.message.clone(),
                    error: None,
                }).into_response();
            }
        }
    }

    let nginx_dir = std::path::PathBuf::from(state.config.read().nginx_dir.clone());
    state.log(format!("Starting nginx download to {}", nginx_dir.display()));
    dl::download_nginx(&nginx_dir, state.downloads.clone());

    Json(AsyncTask { state: "pending".into(), message: "Download started".into(), error: None }).into_response()
}

pub async fn nginx_download_progress(State(state): State<AppStateRef>) -> impl IntoResponse {
    let prog = state.downloads.nginx.lock().clone();
    match prog {
        Some(p) => Json(AsyncTask { state: p.state, message: p.message, error: p.error }).into_response(),
        None => Json(AsyncTask { state: "done".into(), message: "".into(), error: None }).into_response(),
    }
}

pub async fn start_nginx(State(state): State<AppStateRef>) -> impl IntoResponse {
    // nginx::start contains std::thread::sleep — must run on a blocking thread
    let nginx_proc = state.nginx_proc.clone();
    let s = state.clone();
    let result = tokio::task::spawn_blocking(move || nginx_mgr::start(&s, &nginx_proc))
        .await
        .unwrap_or_else(|_| Err("internal error".into()));
    match result {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => {
            state.log(format!("Nginx start failed: {}", e));
            err(StatusCode::INTERNAL_SERVER_ERROR, &e).into_response()
        }
    }
}

pub async fn stop_nginx(State(state): State<AppStateRef>) -> impl IntoResponse {
    // nginx::stop contains std::thread::sleep — must run on a blocking thread
    let nginx_proc = state.nginx_proc.clone();
    let s = state.clone();
    let result = tokio::task::spawn_blocking(move || nginx_mgr::stop(&s, &nginx_proc))
        .await
        .unwrap_or_else(|_| Err("internal error".into()));
    match result {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e).into_response(),
    }
}

pub async fn reload_nginx(State(state): State<AppStateRef>) -> impl IntoResponse {
    match nginx_mgr::reload(&state) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e).into_response(),
    }
}

// ── Services ──────────────────────────────────────────────────────────────────

pub async fn services_status(State(state): State<AppStateRef>) -> impl IntoResponse {
    build_service_status(&state)
}

pub async fn start_services(State(state): State<AppStateRef>) -> axum::response::Response {
    // All blocking work (detect_php_versions spawns child processes; nginx::start has sleeps)
    // goes into a single spawn_blocking so the Tokio worker thread is never stalled.
    let s = state.clone();
    tokio::task::spawn_blocking(move || {
        if s.php_versions.read().is_empty() {
            *s.php_versions.write() = detect_php_versions();
        }
        let default_php = s.config.read().default_php.clone();
        let maybe_v = s.php_versions.read()
            .iter()
            .find(|v| v.major == default_php || v.version.starts_with(&default_php))
            .cloned();
        if let Some(v) = maybe_v {
            let php_proc = s.php_proc.clone();
            if let Err(e) = php_mgr::start(&s, &php_proc, &v) {
                s.log(format!("PHP start warning: {}", e));
            }
        }
        let nginx_proc = s.nginx_proc.clone();
        if let Err(e) = nginx_mgr::start(&s, &nginx_proc) {
            s.log(format!("Nginx start warning: {}", e));
        }
    }).await.ok();

    build_service_status(&state).into_response()
}

pub async fn stop_services(State(state): State<AppStateRef>) -> axum::response::Response {
    // php::stop_all calls child.wait(); nginx::stop has a blocking sleep — both must be
    // on a blocking thread so the Tokio worker is not stalled.
    let s = state.clone();
    tokio::task::spawn_blocking(move || {
        let php_proc = s.php_proc.clone();
        php_mgr::stop_all(&s, &php_proc);
        let nginx_proc = s.nginx_proc.clone();
        let _ = nginx_mgr::stop(&s, &nginx_proc);
    }).await.ok();

    build_service_status(&state).into_response()
}

fn build_service_status(state: &AppStateRef) -> impl IntoResponse {
    let nginx_running = nginx_mgr::is_running(&state.nginx_proc);
    if !nginx_running { state.nginx.write().running = false; }

    let running = php_mgr::running_versions(&state.php_proc);
    let php_status: Vec<PhpVersionStatus> = running.iter().map(|major| {
        let ver = state.php_versions.read().iter()
            .find(|v| &v.major == major)
            .map(|v| v.version.clone())
            .unwrap_or_else(|| major.clone());
        PhpVersionStatus { major: major.clone(), version: ver, running: true }
    }).collect();

    let all = nginx_running && !php_status.is_empty() && php_status.iter().all(|p| p.running);
    Json(ServiceStatus { nginx: nginx_running, php_versions: php_status, all_running: all })
}

// ── Config ────────────────────────────────────────────────────────────────────

pub async fn get_config(State(state): State<AppStateRef>) -> impl IntoResponse {
    Json(state.config.read().clone())
}

pub async fn update_config(
    State(state): State<AppStateRef>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let mut cfg = state.config.write();
    if let Some(v) = body["default_php"].as_str() { cfg.default_php = v.to_string(); }
    if let Some(v) = body["http_port"].as_u64() { cfg.http_port = v as u16; }
    if let Some(v) = body["https_port"].as_u64() { cfg.https_port = v as u16; }
    if let Some(arr) = body["scanned_dirs"].as_array() {
        cfg.scanned_dirs = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
    }
    if let Some(arr) = body["custom_php_dirs"].as_array() {
        cfg.custom_php_dirs = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
    }
    let updated = cfg.clone();
    drop(cfg);
    let base = state.base_dir.clone();
    let _ = updated.save(&base);
    Json(updated)
}

// ── Daemon ────────────────────────────────────────────────────────────────────

pub async fn daemon_logs(State(state): State<AppStateRef>) -> impl IntoResponse {
    let log = state.daemon_log.read();
    let last: Vec<&String> = log.iter().rev().take(100).collect::<Vec<_>>().into_iter().rev().collect();
    Json(serde_json::json!({ "logs": last.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n") }))
}

pub async fn quit_daemon() -> impl IntoResponse {
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        std::process::exit(0);
    });
    Json(serde_json::json!({ "ok": true }))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn detect_project_type(path: &str) -> String {
    let p = std::path::Path::new(path);
    if p.join("artisan").exists() && p.join("public").exists() { return "laravel".into(); }
    if p.join("spark").exists() { return "codeigniter4".into(); }
    if p.join("application").exists() && p.join("system").exists() && p.join("index.php").exists() { return "codeigniter3".into(); }
    if p.join("wp-config.php").exists() || p.join("wp-login.php").exists() { return "wordpress".into(); }
    if p.join("dist").join("index.html").exists() || p.join("build").join("index.html").exists() { return "spa".into(); }
    if p.join("index.html").exists() { return "static".into(); }
    "generic".into()
}


struct PhpInstall { major: String, version: String, binary: String }

fn find_php_binaries() -> Vec<PhpInstall> {
    let mut found = Vec::new();
    let mut search_dirs: Vec<std::path::PathBuf> = vec![];

    // App-managed PHP directory (~/.phpenv/php/{major}/)
    if let Some(home) = dirs_next::home_dir() {
        let app_php = home.join(".phpenv").join("php");
        if app_php.exists() {
            if let Ok(entries) = std::fs::read_dir(&app_php) {
                for e in entries.flatten() {
                    if e.path().is_dir() {
                        search_dirs.push(e.path());
                    }
                }
            }
        }
    }

    // Common Windows PHP locations
    #[cfg(target_os = "windows")]
    {
        for dir in &["C:/xampp/php", "C:/wamp64/bin/php", "C:/laragon/bin/php"] {
            search_dirs.push(std::path::PathBuf::from(dir));
        }
        // wamp multiple versions: C:/wamp64/bin/php/php8.2.x/
        let wamp = std::path::Path::new("C:/wamp64/bin/php");
        if wamp.exists() {
            if let Ok(entries) = std::fs::read_dir(wamp) {
                for e in entries.flatten() {
                    if e.path().is_dir() { search_dirs.push(e.path()); }
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        for dir in &["/usr/bin", "/usr/local/bin"] {
            search_dirs.push(std::path::PathBuf::from(dir));
        }
    }

    let binary_name = if cfg!(target_os = "windows") { "php.exe" } else { "php" };

    for dir in &search_dirs {
        let bin = dir.join(binary_name);
        if !bin.exists() { continue; }
        let binary_path = bin.to_string_lossy().to_string();
        if let Ok(out) = std::process::Command::new(&binary_path).arg("--version").output() {
            let ver_str = String::from_utf8_lossy(&out.stdout);
            if let Some(ver) = parse_php_version(&ver_str) {
                let major = ver.split('.').take(2).collect::<Vec<_>>().join(".");
                let already = found.iter().any(|f: &PhpInstall| f.major == major);
                if !already {
                    found.push(PhpInstall { major, version: ver, binary: binary_path });
                }
            }
        }
    }

    // PATH fallback
    if let Ok(out) = std::process::Command::new("php").arg("--version").output() {
        let ver_str = String::from_utf8_lossy(&out.stdout);
        if let Some(ver) = parse_php_version(&ver_str) {
            let major = ver.split('.').take(2).collect::<Vec<_>>().join(".");
            if !found.iter().any(|f: &PhpInstall| f.major == major) {
                found.push(PhpInstall { major, version: ver, binary: "php".into() });
            }
        }
    }

    found
}

fn parse_php_version(output: &str) -> Option<String> {
    let line = output.lines().next()?;
    if !line.starts_with("PHP ") { return None; }
    let ver = line.split_whitespace().nth(1)?;
    Some(ver.to_string())
}

fn detect_php_versions() -> Vec<super::models::PhpVersion> {
    find_php_binaries().into_iter().map(|p| {
        let fastcgi_port: u16 = {
            let parts: Vec<u16> = p.major.split('.').filter_map(|s| s.parse().ok()).collect();
            if parts.len() >= 2 { 9000 + parts[0] * 10 + parts[1] } else { 9000 }
        };
        super::models::PhpVersion {
            version: p.version,
            major: p.major.clone(),
            binary_path: p.binary.clone(),
            fpm_binary: p.binary,
            fastcgi_addr: format!("127.0.0.1:{}", fastcgi_port),
            installed: true,
            running: false,
        }
    }).collect()
}

fn build_catalog(versions: &[super::models::PhpVersion]) -> Vec<CatalogEntry> {
    let known: &[(&str, &str, bool, bool)] = &[
        ("8.5", "8.5.6",  false, false),
        ("8.4", "8.4.21", false, false),
        ("8.3", "8.3.31", false, false),
        ("8.2", "8.2.31", false, false),
        ("8.1", "8.1.34", true,  false),
        ("8.0", "8.0.30", false, true),
        ("7.4", "7.4.33", false, true),
    ];
    known.iter().map(|(major, latest, security_only, eol)| {
        let installed_ver = versions.iter().find(|v| v.major == *major);
        CatalogEntry {
            major: major.to_string(),
            latest_patch: latest.to_string(),
            installed_patch: installed_ver.map(|v| v.version.clone()),
            installed: installed_ver.is_some(),
            running: false,
            has_update: false,
            security_only: *security_only,
            end_of_life: *eol,
        }
    }).collect()
}
