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
use super::ssl as ssl_mgr;
use super::site_info;
use super::state::save_sites;
use super::validate;

/// Alias del estado del router axum — ahora es `AppContainer` en lugar de
/// `AppState`. Los handlers existentes usan `state.sites`, `state.config`, etc.
/// directamente gracias al `Deref<Target=AppState>` implementado en `AppContainer`.
pub type AppStateRef = Arc<crate::infrastructure::container::AppContainer>;

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
    let php_versions = php_mgr::running_versions(&state.php_proc);
    Json(DaemonStatus {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        uptime: format!("{}s", uptime),
        os: std::env::consts::OS.into(),
        php_versions,
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
    // Generate nginx config BEFORE inserting into state — if this fails the
    // site never enters state and the caller gets a clean error (no partial state).
    site_config::ensure_fastcgi_params(&nginx_dir);
    if let Err(e) = site_config::generate(&site, &nginx_dir, http_port) {
        return err(StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Failed to generate nginx config: {}", e)).into_response();
    }
    if let Err(e) = dns::add_entry(&site.domain) {
        state.log(format!("hosts entry error for {}: {}", site.domain, e));
    }

    state.sites.write().insert(site.id.clone(), site.clone());
    let _ = save_sites(&state);
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

pub async fn enable_ssl(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let site = match state.sites.read().get(&id).cloned() {
        Some(s) => s,
        None => return err(StatusCode::NOT_FOUND, "site not found").into_response(),
    };

    // Mark as pending so the frontend can start polling /ssl/progress
    ssl_mgr::set_task(&state.ssl_tasks, &id, "pending", "Starting SSL issuance…", None);

    let state2   = state.clone();
    let site_id  = id.clone();

    tokio::task::spawn_blocking(move || {
        let set = |s: &str, m: &str, e: Option<String>| {
            ssl_mgr::set_task(&state2.ssl_tasks, &site_id, s, m, e);
        };

        let (base_dir, nginx_dir, certs_dir, http_port) = {
            let cfg = state2.config.read();
            (
                std::path::PathBuf::from(&cfg.base_dir),
                cfg.nginx_dir.clone(),
                cfg.certs_dir.clone(),
                cfg.http_port,
            )
        };

        // 1. Ensure mkcert is present
        set("running", "Downloading mkcert…", None);
        let mkcert = match ssl_mgr::ensure_mkcert(&base_dir) {
            Ok(p) => p,
            Err(e) => { set("error", &e, Some(e.clone())); return; }
        };

        // 2. Install local CA (idempotent)
        set("running", "Installing local CA…", None);
        if let Err(e) = ssl_mgr::install_ca(&mkcert) {
            set("error", &e, Some(e.clone())); return;
        }

        // 3. Issue cert for the domain
        set("running", &format!("Issuing cert for {}…", site.domain), None);
        let certs_path = std::path::Path::new(&certs_dir);
        if let Err(e) = ssl_mgr::issue_cert(&mkcert, &site.domain, certs_path) {
            set("error", &e, Some(e.clone())); return;
        }

        // 4. Update site state + regenerate nginx config
        {
            let mut sites = state2.sites.write();
            if let Some(s) = sites.get_mut(&site_id) {
                s.ssl_enabled = true;
                s.updated_at = chrono::Utc::now().to_rfc3339();
            }
        }
        let _ = save_sites(&state2);

        let updated = state2.sites.read().get(&site_id).cloned();
        if let Some(s) = updated {
            if let Err(e) = site_config::generate_with_certs(&s, &nginx_dir, http_port, Some(&certs_dir)) {
                state2.log(format!("SSL nginx config error: {}", e));
            }
            if nginx_mgr::is_running(&state2.nginx_proc) {
                let _ = nginx_mgr::reload(&state2);
            }
            state2.log(format!("SSL enabled: {}", s.domain));
        }

        set("done", "SSL certificate issued and nginx reloaded", None);
    });

    Json(AsyncTask { state: "pending".into(), message: "SSL issuance started".into(), error: None })
        .into_response()
}

pub async fn disable_ssl(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let site = {
        let mut sites = state.sites.write();
        let s = match sites.get_mut(&id) {
            Some(s) => s,
            None => return err(StatusCode::NOT_FOUND, "site not found").into_response(),
        };
        s.ssl_enabled = false;
        s.updated_at = chrono::Utc::now().to_rfc3339();
        s.clone()
    };
    let _ = save_sites(&state);

    // Revoke cert files
    let certs_dir = state.config.read().certs_dir.clone();
    ssl_mgr::revoke_cert(&site.domain, std::path::Path::new(&certs_dir));

    // Regenerate HTTP-only nginx config
    let (nginx_dir, http_port) = {
        let cfg = state.config.read();
        (cfg.nginx_dir.clone(), cfg.http_port)
    };
    if let Err(e) = site_config::generate(&site, &nginx_dir, http_port) {
        state.log(format!("nginx config error after SSL disable: {}", e));
    }
    if nginx_mgr::is_running(&state.nginx_proc) {
        let _ = nginx_mgr::reload(&state);
    }

    // Clear any ssl task state for this site
    state.ssl_tasks.lock().remove(&id);
    state.log(format!("SSL disabled: {}", site.domain));
    Json(site).into_response()
}

pub async fn ssl_progress(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Check active issuance task first
    if let Some(task) = state.ssl_tasks.lock().get(&id).cloned() {
        return Json(task).into_response();
    }
    // Fallback: reflect current site state
    match state.sites.read().get(&id) {
        Some(s) => Json(AsyncTask {
            state:   "done".into(),
            message: if s.ssl_enabled { "SSL active".into() } else { "SSL disabled".into() },
            error:   None,
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
            tokio::task::spawn_blocking(move || {
                #[cfg(target_os = "windows")]
                let _ = std::process::Command::new("explorer").arg(&p).spawn();
                #[cfg(not(target_os = "windows"))]
                let _ = std::process::Command::new("xdg-open").arg(&p).spawn();
            });
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
    let running = php_mgr::running_versions(&state.php_proc);
    Json(build_catalog(&versions, &running))
}

pub async fn detect_php(State(state): State<AppStateRef>) -> impl IntoResponse {
    let state2 = state.clone();
    let versions = tokio::task::spawn_blocking(move || {
        let v = detect_php_versions();
        *state2.php_versions.write() = v.clone();
        v
    }).await.unwrap_or_default();
    state.log(format!("PHP detect: found {} version(s)", versions.len()));
    let running = php_mgr::running_versions(&state.php_proc);
    Json(build_catalog(&versions, &running))
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

/// GET /api/v1/php/versions/:major/ini
/// Returns the list of known extensions and their enabled state from php.ini.
pub async fn get_php_ini(
    State(state): State<AppStateRef>,
    Path(major): Path<String>,
) -> impl IntoResponse {
    let php_dir = state.config.read().php_dir.clone();
    let ini_path = std::path::Path::new(&php_dir).join(&major).join("php.ini");

    if !ini_path.exists() {
        return err(StatusCode::NOT_FOUND, "php.ini not found — install this PHP version first").into_response();
    }

    let content = match std::fs::read_to_string(&ini_path) {
        Ok(c) => c,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    };

    let extensions = parse_php_extensions(&content);
    let settings   = parse_php_settings(&content);

    Json(PhpIniConfig {
        major: major.clone(),
        ini_path: ini_path.to_string_lossy().to_string(),
        extensions,
        settings,
    }).into_response()
}

/// PUT /api/v1/php/versions/:major/ini
/// Body: { "extensions": [{ "name": "mysqli", "enabled": true }, ...] }
pub async fn update_php_ini(
    State(state): State<AppStateRef>,
    Path(major): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let php_dir = state.config.read().php_dir.clone();
    let ini_path = std::path::Path::new(&php_dir).join(&major).join("php.ini");

    if !ini_path.exists() {
        return err(StatusCode::NOT_FOUND, "php.ini not found").into_response();
    }

    let updates: Vec<PhpExtension> = body["extensions"].as_array()
        .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
        .unwrap_or_default();

    let setting_updates: Vec<PhpSetting> = body["settings"].as_array()
        .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
        .unwrap_or_default();

    // Validate all inputs before touching the file
    for ext in &updates {
        if let Err(e) = validate_extension_name(&ext.name) {
            return err(StatusCode::BAD_REQUEST, &e).into_response();
        }
    }
    for setting in &setting_updates {
        if let Err(e) = validate_php_setting(&setting.key, &setting.value) {
            return err(StatusCode::BAD_REQUEST, &e).into_response();
        }
    }

    let content = match std::fs::read_to_string(&ini_path) {
        Ok(c) => c,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    };

    let after_ext      = apply_extension_changes(&content, &updates);
    let new_content    = apply_setting_changes(&after_ext, &setting_updates);

    match std::fs::write(&ini_path, &new_content) {
        Ok(()) => {},
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()).into_response(),
    }

    // Stop PHP-CGI, reload nginx, then restart PHP-CGI so all changes take effect.
    // Order matters: stop PHP first → reload nginx (clears FastCGI connection cache) →
    // restart PHP so it picks up the new php.ini on startup.
    let php_was_running = php_mgr::is_running(&state.php_proc, &major);

    if php_was_running {
        let php_proc = state.php_proc.clone();
        let _ = php_mgr::stop(&state, &php_proc, &major);
        state.log(format!("PHP {} stopped for php.ini reload", major));
    }

    // Reload nginx to drop any cached FastCGI connections to the old php-cgi process
    if nginx_mgr::is_running(&state.nginx_proc) {
        let _ = nginx_mgr::reload(&state);
        state.log("Nginx reloaded after php.ini change".into());
    }

    // Restart PHP-CGI with the new php.ini
    if php_was_running {
        let versions = detect_php_versions();
        *state.php_versions.write() = versions.clone();
        if let Some(v) = versions.iter().find(|v| v.major == major) {
            let php_proc2 = state.php_proc.clone();
            let _ = php_mgr::start(&state, &php_proc2, v);
            state.log(format!("PHP {} restarted with new php.ini", major));
        }
    }

    state.log(format!("php.ini updated for PHP {}", major));

    // Return updated config
    let updated_content = std::fs::read_to_string(&ini_path).unwrap_or_default();
    let extensions = parse_php_extensions(&updated_content);
    let settings   = parse_php_settings(&updated_content);
    Json(PhpIniConfig {
        major,
        ini_path: ini_path.to_string_lossy().to_string(),
        extensions,
        settings,
    }).into_response()
}

/// Parse extension lines from php.ini content.
/// Returns all known extensions with their enabled state.
fn parse_php_extensions(content: &str) -> Vec<PhpExtension> {
    // Canonical list of extensions we track, with categories
    let known: &[(&str, &str)] = &[
        // Database
        ("mysqli",     "database"),
        ("pdo_mysql",  "database"),
        ("pdo_pgsql",  "database"),
        ("pdo_sqlite", "database"),
        ("pdo_oci",    "database"),
        ("oci8_12c",   "database"),
        ("sqlite3",    "database"),
        // String / encoding / i18n
        ("mbstring",   "string"),
        ("iconv",      "string"),
        ("intl",       "string"),
        ("gettext",    "string"),
        ("ctype",      "string"),
        ("pspell",     "string"),
        ("enchant",    "string"),
        // Image
        ("gd",         "image"),
        ("exif",       "image"),
        ("imagick",    "image"),
        // Network / mail
        ("curl",       "network"),
        ("soap",       "network"),
        ("ldap",       "network"),
        ("sockets",    "network"),
        ("ftp",        "network"),
        // Files / compression
        ("zip",        "files"),
        ("zlib",       "files"),
        ("bz2",        "files"),
        ("fileinfo",   "files"),
        // Security / crypto
        ("openssl",    "security"),
        ("sodium",     "security"),
        ("hash",       "security"),
        // Math / misc
        ("bcmath",     "math"),
        ("gmp",        "math"),
        ("calendar",   "misc"),
        ("pcntl",      "misc"),
        ("shmop",      "misc"),
        ("sysvmsg",    "misc"),
        ("sysvsem",    "misc"),
        ("sysvshm",    "misc"),
        ("xml",        "misc"),
        ("xmlrpc",     "misc"),
        ("xsl",        "misc"),
        ("dom",        "misc"),
        ("simplexml",  "misc"),
        ("tokenizer",  "misc"),
    ];

    known.iter().map(|(name, category)| {
        // An extension is enabled if there's an uncommented `extension=name` line
        let enabled = content.lines().any(|line| {
            let t = line.trim();
            !t.starts_with(';') && (
                t == format!("extension={}", name) ||
                t.starts_with(&format!("extension={} ", name)) ||
                t.starts_with(&format!("extension={};", name))
            )
        });
        PhpExtension {
            name: name.to_string(),
            enabled,
            category: category.to_string(),
        }
    }).collect()
}

/// Apply enable/disable changes to php.ini content.
fn apply_extension_changes(content: &str, updates: &[PhpExtension]) -> String {
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

    for ext in updates {
        let ext_line  = format!("extension={}", ext.name);
        let commented = format!(";extension={}", ext.name);

        // Find existing line for this extension
        let pos = lines.iter().position(|l| {
            let t = l.trim();
            t == ext_line || t.starts_with(&format!("{}=", "extension")) && t.contains(&ext.name) ||
            t == commented || t.starts_with(&format!(";extension={}", ext.name))
        });

        if let Some(idx) = pos {
            if ext.enabled {
                lines[idx] = ext_line.clone();
            } else {
                lines[idx] = format!(";{}", ext_line);
            }
        } else if ext.enabled {
            // Extension not present at all — add it at the end of the [extensions] section
            // or just at the end of the file
            lines.push(ext_line.clone());
        }
    }

    let mut result = lines.join("\n");
    if content.ends_with('\n') { result.push('\n'); }
    result
}

/// The php.ini settings we expose in the UI, with labels and hints.
fn known_settings() -> &'static [(&'static str, &'static str, &'static str)] {
    &[
        ("max_input_vars",      "Max Input Vars",       "Max number of form fields (default 1000). Increase for pages with many checkboxes/permissions."),
        ("post_max_size",       "Max POST Size",        "Max size of POST data, e.g. 8M, 64M. Must be >= upload_max_filesize."),
        ("upload_max_filesize", "Max Upload Size",      "Max size of a single uploaded file, e.g. 2M, 64M."),
        ("memory_limit",        "Memory Limit",         "PHP memory limit per request, e.g. 128M, 512M."),
        ("max_execution_time",  "Max Execution Time",   "Max time (seconds) a script can run. 0 = unlimited."),
        ("max_input_time",      "Max Input Time",       "Max time (seconds) to parse request data."),
        ("error_reporting",     "Error Reporting",      "PHP error reporting level, e.g. E_ALL, E_ALL & ~E_NOTICE."),
        ("display_errors",      "Display Errors",       "Show errors in browser output: On | Off."),
        ("log_errors",          "Log Errors",           "Write errors to log file: On | Off."),
        ("date.timezone",       "Timezone",             "PHP timezone, e.g. America/Lima, UTC, Europe/Madrid."),
        ("default_charset",     "Default Charset",      "Default character encoding for HTTP responses, e.g. UTF-8, ISO-8859-1."),
        ("intl.default_locale", "Default Locale",       "ICU locale for intl extension, e.g. es_PE, en_US, pt_BR."),
    ]
}

/// Parse known settings from php.ini content, returning their current value.
fn parse_php_settings(content: &str) -> Vec<PhpSetting> {
    known_settings().iter().map(|(key, label, hint)| {
        // Match both `key = value` and `;key = value` (commented)
        let value = content.lines()
            .filter(|l| !l.trim().starts_with(';'))
            .find_map(|line| {
                let t = line.trim();
                let prefix = format!("{} =", key);
                let prefix2 = format!("{}=", key);
                if t.starts_with(&prefix) {
                    Some(t[prefix.len()..].trim().to_string())
                } else if t.starts_with(&prefix2) {
                    Some(t[prefix2.len()..].trim().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_default();

        PhpSetting {
            key: key.to_string(),
            value,
            label: label.to_string(),
            hint: hint.to_string(),
        }
    }).collect()
}

/// Validate a php.ini setting key and value before writing.
/// Returns Err with a human-readable message if the value is rejected.
fn validate_php_setting(key: &str, value: &str) -> Result<(), String> {
    // Key must be in the known whitelist — prevents injection of arbitrary directives
    let allowed_keys: Vec<&str> = known_settings().iter().map(|(k, _, _)| *k).collect();
    if !allowed_keys.contains(&key) {
        return Err(format!("Setting '{}' is not allowed", key));
    }

    // Empty value = revert to default (always allowed)
    if value.is_empty() { return Ok(()); }

    // Reject values that contain newlines or null bytes — would break the ini file
    if value.contains('\n') || value.contains('\r') || value.contains('\0') {
        return Err(format!("Value for '{}' contains illegal characters", key));
    }

    match key {
        // Integer-only settings
        "max_input_vars" | "max_execution_time" | "max_input_time" => {
            if !value.chars().all(|c| c.is_ascii_digit()) {
                return Err(format!("'{}' must be a non-negative integer (got '{}')", key, value));
            }
        }
        // Byte-size settings: digits followed by optional K/M/G
        "post_max_size" | "upload_max_filesize" | "memory_limit" => {
            let upper = value.to_uppercase();
            let (digits, suffix) = match upper.chars().last() {
                Some('K') | Some('M') | Some('G') => (&value[..value.len()-1], true),
                _ => (value, false),
            };
            let _ = suffix; // suffix is valid, we just need to check the digits part
            if !digits.chars().all(|c| c.is_ascii_digit()) || digits.is_empty() {
                return Err(format!("'{}' must be a size like 128M, 1G, 2048K (got '{}')", key, value));
            }
        }
        // Boolean settings
        "display_errors" | "log_errors" => {
            match value.to_lowercase().as_str() {
                "on" | "off" | "1" | "0" | "true" | "false" => {}
                _ => return Err(format!("'{}' must be On or Off (got '{}')", key, value)),
            }
        }
        // Timezone: letters, digits, slash, underscore, hyphen, dot only
        "date.timezone" => {
            if !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '_' | '-' | '.')) {
                return Err(format!("'{}' contains invalid characters for a timezone (got '{}')", key, value));
            }
        }
        // Error reporting: PHP constant expression — allow alphanumeric, spaces, and safe operators
        "error_reporting" => {
            let allowed: fn(char) -> bool = |c: char| {
                c.is_ascii_alphanumeric() || matches!(c, ' ' | '|' | '&' | '~' | '^' | '_')
            };
            if !value.chars().all(allowed) {
                return Err(format!("'{}' contains invalid characters (got '{}')", key, value));
            }
        }
        // Charset name: letters, digits, hyphens, underscores (e.g. UTF-8, ISO-8859-1)
        "default_charset" => {
            if !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')) {
                return Err(format!("'{}' must be a valid charset name like UTF-8, ISO-8859-1 (got '{}')", key, value));
            }
        }
        // ICU locale: letters, digits, underscores, hyphens (e.g. es_PE, en_US, pt_BR)
        "intl.default_locale" => {
            if !value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-')) {
                return Err(format!("'{}' must be a locale like es_PE, en_US, pt_BR (got '{}')", key, value));
            }
        }
        _ => {}
    }
    Ok(())
}

/// Validate an extension name against the known whitelist.
fn validate_extension_name(name: &str) -> Result<(), String> {
    // Extension names: lowercase letters, digits, underscores only
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("Extension name '{}' contains invalid characters", name));
    }
    Ok(())
}

/// Write setting changes into php.ini content.
fn apply_setting_changes(content: &str, updates: &[PhpSetting]) -> String {
    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();

    for setting in updates {
        // Skip settings that fail validation — they were already checked in the handler
        if validate_php_setting(&setting.key, &setting.value).is_err() { continue; }

        let new_line = format!("{} = {}", setting.key, setting.value);

        // Find an existing line (commented or not) for this key
        let pos = lines.iter().position(|l| {
            let t = l.trim().trim_start_matches(';').trim();
            t.starts_with(&format!("{} =", setting.key)) ||
            t.starts_with(&format!("{}=", setting.key))
        });

        if let Some(idx) = pos {
            if setting.value.is_empty() {
                // Empty value = comment out (revert to default)
                lines[idx] = format!(";{}", new_line);
            } else {
                lines[idx] = new_line;
            }
        } else if !setting.value.is_empty() {
            // Not found — append at end of file
            lines.push(new_line);
        }
    }

    let mut result = lines.join("\n");
    if content.ends_with('\n') { result.push('\n'); }
    result
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

fn build_catalog(versions: &[super::models::PhpVersion], running: &[String]) -> Vec<CatalogEntry> {
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
        let is_running = running.iter().any(|r| r == *major);
        let has_update = installed_ver.map(|v| v.version != *latest).unwrap_or(false);
        CatalogEntry {
            major: major.to_string(),
            latest_patch: latest.to_string(),
            installed_patch: installed_ver.map(|v| v.version.clone()),
            installed: installed_ver.is_some(),
            running: is_running,
            has_update,
            security_only: *security_only,
            end_of_life: *eol,
        }
    }).collect()
}
