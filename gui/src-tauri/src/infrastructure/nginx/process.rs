use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use parking_lot::Mutex;
use crate::infrastructure::{state::AppState, nginx::vhost_config};

pub struct NginxProcess { pub child: Mutex<Option<Child>> }
impl NginxProcess { pub fn new() -> Arc<Self> { Arc::new(Self { child: Mutex::new(None) }) } }

pub fn find_nginx_binary(nginx_dir: &str) -> Option<PathBuf> {
    let win = Path::new(nginx_dir).join("nginx.exe");
    if win.exists() { return Some(win); }
    for p in &["/usr/sbin/nginx", "/usr/local/sbin/nginx", "/opt/homebrew/bin/nginx"] {
        if Path::new(p).exists() { return Some(PathBuf::from(p)); }
    }
    which::which("nginx").ok()
}

pub fn nginx_prefix(binary: &Path) -> PathBuf {
    let bin_dir = binary.parent().unwrap_or(binary);
    if bin_dir.join("conf").join("nginx.conf").exists() { return bin_dir.to_path_buf(); }
    if bin_dir.join("nginx.conf").exists() { return bin_dir.to_path_buf(); }
    for p in &["/etc/nginx", "/usr/local/etc/nginx"] { if Path::new(p).exists() { return PathBuf::from(p); } }
    bin_dir.to_path_buf()
}

pub fn get_nginx_version(binary: &Path) -> Option<String> {
    let out  = Command::new(binary).arg("-v").output().ok()?;
    let line = String::from_utf8_lossy(&out.stderr).lines().find(|l| l.contains("nginx/"))?.to_string();
    Some(line.split('/').nth(1)?.trim().to_string())
}

pub fn start(state: &AppState, nginx_proc: &Arc<NginxProcess>) -> Result<(), String> {
    let config        = state.config.read();
    let binary        = find_nginx_binary(&config.nginx_dir).ok_or_else(|| "nginx binary not found".to_string())?;
    let http_port     = config.http_port;
    let https_port    = config.https_port;
    let nginx_dir     = config.nginx_dir.clone();
    let certs_dir     = config.certs_dir.clone();
    drop(config);

    test_binary(&binary)?;

    // Regenerar configs de todos los sitios antes de arrancar.
    // Se usa generate_with_certs para TODOS para que los sitios con SSL
    // apunten a certs_dir (~/.phpenv/certs/) en lugar de nginx_dir.
    {
        let sites = state.sites.read().values().cloned().collect::<Vec<_>>();
        vhost_config::ensure_fastcgi_params(&nginx_dir);
        for site in &sites {
            let _ = vhost_config::generate_with_certs(site, &nginx_dir, http_port, Some(&certs_dir));
        }
    }

    ensure_config(&nginx_dir, &binary, http_port, https_port)?;
    let conf_path = PathBuf::from(&nginx_dir).join("nginx.conf");
    test_config(&binary, &conf_path)?;

    let mut lock = nginx_proc.child.lock();
    if let Some(ref mut existing) = *lock {
        if existing.try_wait().map(|s| s.is_none()).unwrap_or(false) { return Err("nginx is already running".into()); }
        *lock = None;
    }

    let bin_dir   = binary.parent().unwrap_or(&binary);
    let error_log = PathBuf::from(&nginx_dir).join("logs").join("error.log");
    let child     = Command::new(&binary).current_dir(bin_dir)
        .args(["-c", &conf_path.to_string_lossy(), "-e", &error_log.to_string_lossy()])
        .spawn().map_err(|e| format!("Failed to spawn nginx: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(300));
    let version = get_nginx_version(&binary);
    let pid     = child.id();
    *lock = Some(child);

    if let Some(ref mut c) = *lock {
        if let Ok(Some(status)) = c.try_wait() { *lock = None; return Err(format!("nginx exited immediately (code {:?})", status.code())); }
    }
    drop(lock);

    let mut nginx = state.nginx.write();
    nginx.running = true; nginx.pid = Some(pid); nginx.version = version;
    state.log("Nginx started".into());
    Ok(())
}

pub fn stop(state: &AppState, nginx_proc: &Arc<NginxProcess>) -> Result<(), String> {
    let binary = { let cfg = state.config.read(); find_nginx_binary(&cfg.nginx_dir) };
    if let Some(bin) = &binary { let _ = Command::new(bin).args(["-s", "quit"]).output(); }
    let mut lock = nginx_proc.child.lock();
    if let Some(mut child) = lock.take() {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = child.kill(); let _ = child.wait();
    }
    let mut nginx = state.nginx.write();
    nginx.running = false; nginx.pid = None;
    state.log("Nginx stopped".into());
    Ok(())
}

pub fn reload(state: &AppState) -> Result<(), String> {
    let (binary, conf_path) = { let cfg = state.config.read(); (find_nginx_binary(&cfg.nginx_dir), PathBuf::from(&cfg.nginx_dir).join("nginx.conf")) };
    let binary = binary.ok_or_else(|| "nginx binary not found".to_string())?;
    let bin_dir = binary.parent().unwrap_or(&binary);
    let out = Command::new(&binary).current_dir(bin_dir).args(["-s", "reload", "-c", &conf_path.to_string_lossy()]).output().map_err(|e| e.to_string())?;
    if out.status.success() { state.log("Nginx reloaded".into()); Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).to_string()) }
}

pub fn is_running(nginx_proc: &Arc<NginxProcess>) -> bool {
    let mut lock = nginx_proc.child.lock();
    if let Some(child) = lock.as_mut() {
        match child.try_wait() { Ok(None) => true, Ok(Some(_)) | Err(_) => { *lock = None; false } }
    } else { false }
}

fn test_binary(binary: &Path) -> Result<(), String> {
    let out = Command::new(binary).arg("-v").output().map_err(|e| format!("Cannot run nginx: {}", e))?;
    if !out.status.success() { return Err(format!("nginx binary error: {}", String::from_utf8_lossy(&out.stderr).trim())); }
    Ok(())
}

fn test_config(binary: &Path, conf: &Path) -> Result<(), String> {
    let bin_dir = binary.parent().unwrap_or(binary);
    let out = Command::new(binary).current_dir(bin_dir).args(["-t", "-c", &conf.to_string_lossy()]).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).to_string()) }
}

fn ensure_config(nginx_dir: &str, _binary: &Path, http_port: u16, https_port: u16) -> Result<(), String> {
    let dir = Path::new(nginx_dir);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.join("sites")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.join("logs")).map_err(|e| e.to_string())?;
    let mime_path = dir.join("mime.types");
    std::fs::write(&mime_path, BASIC_MIME_TYPES).map_err(|e| e.to_string())?;
    let conf_path = dir.join("nginx.conf");
    if conf_path.exists() { return Ok(()); }
    let to_fwd = |p: PathBuf| p.to_string_lossy().replace('\\', "/");
    let mime  = to_fwd(mime_path); let logs = to_fwd(dir.join("logs")); let sites = to_fwd(dir.join("sites")); let _ = https_port;
    std::fs::write(conf_path, format!(r#"worker_processes 1;
error_log  "{logs}/error.log";
pid        "{logs}/nginx.pid";
events {{ worker_connections 1024; }}
http {{ include "{mime}"; default_type application/octet-stream; access_log "{logs}/access.log"; sendfile on; keepalive_timeout 65; server {{ listen {http_port} default_server; server_name _; return 444; }} include "{sites}/*.conf"; }}
"#)).map_err(|e| e.to_string())
}

const BASIC_MIME_TYPES: &str = "types {\n    text/html html htm shtml;\n    text/css css;\n    text/xml xml;\n    application/javascript js;\n    application/json json;\n    image/gif gif;\n    image/jpeg jpeg jpg;\n    image/png png;\n    image/svg+xml svg svgz;\n    image/webp webp;\n    image/x-icon ico;\n    font/woff woff;\n    font/woff2 woff2;\n    application/octet-stream bin exe dll;\n    application/zip zip;\n    application/pdf pdf;\n}\n";
