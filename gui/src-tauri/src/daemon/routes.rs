use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use uuid::Uuid;

use super::download as dl;
use super::models::*;
use super::nginx as nginx_mgr;
use super::state::{AppState, save_sites};

pub type AppStateRef = Arc<AppState>;

fn err(status: StatusCode, msg: &str) -> impl IntoResponse {
    (status, Json(serde_json::json!({ "error": msg })))
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
    let now = chrono::Utc::now().to_rfc3339();
    let project_type = detect_project_type(&path);
    let php_version = state.config.read().default_php.clone();
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
    Json(updated).into_response()
}

pub async fn delete_site(
    State(state): State<AppStateRef>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.sites.write().remove(&id) {
        Some(s) => {
            let _ = save_sites(&state);
            state.log(format!("Site deleted: {}", s.domain));
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        None => err(StatusCode::NOT_FOUND, "site not found").into_response(),
    }
}

pub async fn scan_sites(State(state): State<AppStateRef>) -> impl IntoResponse {
    let scan_dirs = state.config.read().scanned_dirs.clone();
    let custom_dirs = state.config.read().custom_php_dirs.clone();
    let mut added = 0u32;

    for dir in scan_dirs.iter().chain(custom_dirs.iter()) {
        let p = std::path::Path::new(dir);
        if let Ok(entries) = std::fs::read_dir(p) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() { continue; }
                let domain = format!("{}.test", path.file_name().unwrap_or_default().to_string_lossy());
                let already = state.sites.read().values().any(|s| s.domain == domain);
                if already { continue; }
                let now = chrono::Utc::now().to_rfc3339();
                let path_str = path.to_string_lossy().to_string();
                let project_type = detect_project_type(&path_str);
                let php_version = state.config.read().default_php.clone();
                let site = Site {
                    id: Uuid::new_v4().to_string(),
                    name: domain.clone(),
                    domain,
                    path: path_str,
                    project_type,
                    php_version,
                    ssl_enabled: false,
                    active: false,
                    created_at: now.clone(),
                    updated_at: now,
                };
                state.sites.write().insert(site.id.clone(), site);
                added += 1;
            }
        }
    }

    let _ = save_sites(&state);
    state.log(format!("Scan complete: {} new sites found", added));

    let sites = state.sites.read();
    let mut list: Vec<Site> = sites.values().cloned().collect();
    list.sort_by(|a, b| a.domain.cmp(&b.domain));
    Json(list)
}

pub async fn bulk_add_sites(
    State(state): State<AppStateRef>,
    Json(body): Json<Vec<serde_json::Value>>,
) -> impl IntoResponse {
    for item in body {
        let domain = item["domain"].as_str().unwrap_or("").to_string();
        let path = item["path"].as_str().unwrap_or("").to_string();
        if domain.is_empty() || path.is_empty() { continue; }
        let already = state.sites.read().values().any(|s| s.domain == domain);
        if already { continue; }
        let now = chrono::Utc::now().to_rfc3339();
        let project_type = detect_project_type(&path);
        let php_version = state.config.read().default_php.clone();
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
        state.sites.write().insert(site.id.clone(), site);
    }
    let _ = save_sites(&state);
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
    match state.sites.read().get(&id).cloned() {
        Some(s) => { state.log(format!("Config refreshed: {}", s.domain)); Json(s).into_response() }
        None => err(StatusCode::NOT_FOUND, "site not found").into_response(),
    }
}

pub async fn get_site_info(
    State(_state): State<AppStateRef>,
    Path(_id): Path<String>,
) -> impl IntoResponse {
    Json(serde_json::json!({
        "app_name": "",
        "app_env": "",
        "app_debug": false,
        "app_url": "",
        "app_timezone": "",
        "app_locale": "",
        "framework_name": "",
        "framework_version": "",
        "maintenance_mode": false
    }))
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
    let cached = state.php_versions.read();
    if !cached.is_empty() {
        return Json(cached.clone());
    }
    drop(cached);
    let versions = detect_php_versions();
    *state.php_versions.write() = versions.clone();
    Json(versions)
}

pub async fn php_catalog(State(state): State<AppStateRef>) -> impl IntoResponse {
    let cached = state.php_versions.read();
    let versions = if cached.is_empty() {
        drop(cached);
        let v = detect_php_versions();
        *state.php_versions.write() = v.clone();
        v
    } else {
        cached.clone()
    };
    Json(build_catalog(&versions))
}

pub async fn detect_php(State(state): State<AppStateRef>) -> impl IntoResponse {
    let versions = detect_php_versions();
    *state.php_versions.write() = versions.clone();
    state.log(format!("PHP detect: found {} version(s)", versions.len()));
    Json(build_catalog(&versions))
}

pub async fn install_php(Json(_body): Json<serde_json::Value>) -> impl IntoResponse {
    (StatusCode::NOT_IMPLEMENTED, Json(serde_json::json!({ "error": "PHP install not yet implemented in Rust daemon" })))
}

pub async fn install_php_progress(Path(major): Path<String>) -> impl IntoResponse {
    Json(InstallProgress {
        major,
        state: "done".into(),
        message: "Not available".into(),
        percent: 100,
        error: None,
    })
}

pub async fn start_php_fpm(Path(_version): Path<String>) -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true }))
}

pub async fn stop_php_fpm(Path(_version): Path<String>) -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true }))
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
    let nginx_proc = state.nginx_proc.clone();
    match nginx_mgr::start(&state, &nginx_proc) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => {
            state.log(format!("Nginx start failed: {}", e));
            err(StatusCode::INTERNAL_SERVER_ERROR, &e).into_response()
        }
    }
}

pub async fn stop_nginx(State(state): State<AppStateRef>) -> impl IntoResponse {
    let nginx_proc = state.nginx_proc.clone();
    match nginx_mgr::stop(&state, &nginx_proc) {
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
    let nginx = state.nginx.read();
    Json(ServiceStatus {
        nginx: nginx.running,
        php_versions: vec![],
        all_running: nginx.running,
    })
}

pub async fn start_services(State(state): State<AppStateRef>) -> impl IntoResponse {
    state.log("Services start requested".into());
    Json(ServiceStatus { nginx: false, php_versions: vec![], all_running: false })
}

pub async fn stop_services(State(state): State<AppStateRef>) -> impl IntoResponse {
    state.log("Services stop requested".into());
    Json(ServiceStatus { nginx: false, php_versions: vec![], all_running: false })
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
    if p.join("wp-config.php").exists() || p.join("wp-login.php").exists() { return "wordpress".into(); }
    if p.join("dist").join("index.html").exists() || p.join("build").join("index.html").exists() { return "spa".into(); }
    if p.join("index.html").exists() { return "static".into(); }
    "generic".into()
}


struct PhpInstall { major: String, version: String, binary: String }

fn find_php_binaries() -> Vec<PhpInstall> {
    let mut found = Vec::new();
    let candidates: &[&str] = if cfg!(target_os = "windows") {
        &[
            "C:/xampp/php", "C:/wamp64/bin/php", "C:/laragon/bin/php",
            "C:/Program Files/PHP",
        ]
    } else {
        &["/usr/bin", "/usr/local/bin"]
    };

    for dir in candidates {
        let p = std::path::Path::new(dir);
        if !p.exists() { continue; }
        if let Ok(entries) = std::fs::read_dir(p) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let binary = if cfg!(target_os = "windows") { "php.exe" } else { "php" };
                if name == binary || name.starts_with("php") {
                    let binary_path = entry.path().to_string_lossy().to_string();
                    if let Ok(out) = std::process::Command::new(&binary_path).arg("--version").output() {
                        let ver_str = String::from_utf8_lossy(&out.stdout);
                        if let Some(ver) = parse_php_version(&ver_str) {
                            let major = ver.split('.').take(2).collect::<Vec<_>>().join(".");
                            found.push(PhpInstall { major, version: ver, binary: binary_path });
                        }
                    }
                }
            }
        }
    }

    // Also check PATH
    if let Ok(out) = std::process::Command::new("php").arg("--version").output() {
        let ver_str = String::from_utf8_lossy(&out.stdout);
        if let Some(ver) = parse_php_version(&ver_str) {
            let major = ver.split('.').take(2).collect::<Vec<_>>().join(".");
            let already = found.iter().any(|f| f.major == major);
            if !already {
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
        ("8.4", "8.4.8",  false, false),
        ("8.3", "8.3.22", false, false),
        ("8.2", "8.2.29", false, false),
        ("8.1", "8.1.32", true,  false),
        ("8.0", "8.0.30", false, true),
        ("7.4", "7.4.33", false, true),
        ("7.3", "7.3.33", false, true),
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
