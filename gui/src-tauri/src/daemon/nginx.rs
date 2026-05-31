use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use parking_lot::Mutex;

use super::state::AppState;

pub struct NginxProcess {
    pub child: Mutex<Option<Child>>,
}

impl NginxProcess {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { child: Mutex::new(None) })
    }
}

pub fn find_nginx_binary(nginx_dir: &str) -> Option<PathBuf> {
    let win = Path::new(nginx_dir).join("nginx.exe");
    if win.exists() { return Some(win); }

    for p in &["/usr/sbin/nginx", "/usr/local/sbin/nginx", "/opt/homebrew/bin/nginx"] {
        if Path::new(p).exists() { return Some(PathBuf::from(p)); }
    }

    which::which("nginx").ok()
}

/// Infer the nginx prefix directory (where conf/, logs/, html/ live)
/// from the binary location. For a downloaded nginx on Windows the binary
/// is at <nginx_dir>/nginx.exe so the prefix is <nginx_dir>.
/// For a system install (/usr/sbin/nginx) the prefix is /etc/nginx or similar.
pub fn nginx_prefix(binary: &Path) -> PathBuf {
    let bin_dir = binary.parent().unwrap_or(binary);
    // If nginx.conf exists next to the binary, that's the prefix
    if bin_dir.join("conf").join("nginx.conf").exists() {
        return bin_dir.to_path_buf();
    }
    if bin_dir.join("nginx.conf").exists() {
        return bin_dir.to_path_buf();
    }
    // System nginx: prefix is typically /etc/nginx
    for p in &["/etc/nginx", "/usr/local/etc/nginx"] {
        if Path::new(p).exists() { return PathBuf::from(p); }
    }
    bin_dir.to_path_buf()
}

pub fn get_nginx_version(binary: &Path) -> Option<String> {
    let out = Command::new(binary).arg("-v").output().ok()?;
    let stderr = String::from_utf8_lossy(&out.stderr);
    let line = stderr.lines().find(|l| l.contains("nginx/"))?;
    Some(line.split('/').nth(1)?.trim().to_string())
}

pub fn start(state: &Arc<AppState>, nginx_proc: &Arc<NginxProcess>) -> Result<(), String> {
    let config = state.config.read();
    let binary = find_nginx_binary(&config.nginx_dir)
        .ok_or_else(|| "nginx binary not found".to_string())?;
    let http_port = config.http_port;
    let https_port = config.https_port;
    let nginx_dir = config.nginx_dir.clone();
    let config_nginx_dir = nginx_dir.clone();
    drop(config);

    // Validate the binary actually works
    test_binary(&binary)?;

    // Ensure our config and directory structure exist
    ensure_config(&nginx_dir, &binary, http_port, https_port)?;

    let conf_path = PathBuf::from(&nginx_dir).join("nginx.conf");

    // Test config before starting
    test_config(&binary, &conf_path)?;

    let mut lock = nginx_proc.child.lock();
    if let Some(ref mut existing) = *lock {
        if existing.try_wait().map(|s| s.is_none()).unwrap_or(false) {
            return Err("nginx is already running".into());
        }
        *lock = None;
    }

    // On Windows nginx must be run from its own directory so it can find
    // relative paths (html/, logs/, conf/mime.types, etc.)
    let bin_dir = binary.parent().unwrap_or(&binary);
    let error_log = PathBuf::from(&config_nginx_dir).join("logs").join("error.log");
    let child = Command::new(&binary)
        .current_dir(bin_dir)
        .args([
            "-c", &conf_path.to_string_lossy(),
            "-e", &error_log.to_string_lossy(),
        ])
        .spawn()
        .map_err(|e| format!("Failed to spawn nginx: {}", e))?;

    // Give nginx a moment to start then check it didn't immediately crash
    std::thread::sleep(std::time::Duration::from_millis(300));

    let version = get_nginx_version(&binary);
    let pid = child.id();
    *lock = Some(child);

    // Check if it's still alive after startup
    if let Some(ref mut c) = *lock {
        match c.try_wait() {
            Ok(Some(status)) => {
                *lock = None;
                return Err(format!("nginx exited immediately (code {:?}) — check config", status.code()));
            }
            _ => {}
        }
    }
    drop(lock);

    let mut nginx = state.nginx.write();
    nginx.running = true;
    nginx.pid = Some(pid);
    nginx.version = version;

    state.log("Nginx started".into());
    Ok(())
}

pub fn stop(state: &Arc<AppState>, nginx_proc: &Arc<NginxProcess>) -> Result<(), String> {
    let config = state.config.read();
    let binary = find_nginx_binary(&config.nginx_dir);
    drop(config);

    if let Some(bin) = &binary {
        let _ = Command::new(bin).args(["-s", "quit"]).output();
    }

    let mut lock = nginx_proc.child.lock();
    if let Some(mut child) = lock.take() {
        // Give quit signal time to work, then kill
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = child.kill();
        let _ = child.wait();
    }

    let mut nginx = state.nginx.write();
    nginx.running = false;
    nginx.pid = None;

    state.log("Nginx stopped".into());
    Ok(())
}

pub fn reload(state: &Arc<AppState>) -> Result<(), String> {
    let config = state.config.read();
    let binary = find_nginx_binary(&config.nginx_dir)
        .ok_or_else(|| "nginx binary not found".to_string())?;
    let conf_path = PathBuf::from(&config.nginx_dir).join("nginx.conf");
    drop(config);

    let bin_dir = binary.parent().unwrap_or(&binary);
    let out = Command::new(&binary)
        .current_dir(bin_dir)
        .args(["-s", "reload", "-c", &conf_path.to_string_lossy()])
        .output()
        .map_err(|e| e.to_string())?;

    if out.status.success() {
        state.log("Nginx reloaded".into());
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

pub fn is_running(nginx_proc: &Arc<NginxProcess>) -> bool {
    let mut lock = nginx_proc.child.lock();
    if let Some(child) = lock.as_mut() {
        match child.try_wait() {
            Ok(None) => true,
            Ok(Some(_)) | Err(_) => { *lock = None; false }
        }
    } else {
        false
    }
}

fn test_binary(binary: &Path) -> Result<(), String> {
    let out = Command::new(binary).arg("-v").output()
        .map_err(|e| format!("Cannot run nginx: {}", e))?;
    // nginx -v writes to stderr, exit 0
    if out.stderr.is_empty() && !out.status.success() {
        return Err("nginx binary did not respond".into());
    }
    Ok(())
}

fn test_config(binary: &Path, conf: &Path) -> Result<(), String> {
    let bin_dir = binary.parent().unwrap_or(binary);
    let out = Command::new(binary)
        .current_dir(bin_dir)
        .args(["-t", "-c", &conf.to_string_lossy()])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

fn ensure_config(nginx_dir: &str, _binary: &Path, http_port: u16, https_port: u16) -> Result<(), String> {
    let dir = Path::new(nginx_dir);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.join("sites")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.join("logs")).map_err(|e| e.to_string())?;

    // Always write our own mime.types so we control its location
    let mime_path = dir.join("mime.types");
    std::fs::write(&mime_path, BASIC_MIME_TYPES).map_err(|e| e.to_string())?;

    let conf_path = dir.join("nginx.conf");
    if conf_path.exists() { return Ok(()); }

    // Use forward slashes — nginx on Windows accepts them fine
    let to_fwd = |p: PathBuf| p.to_string_lossy().replace('\\', "/");
    let mime   = to_fwd(mime_path);
    let logs   = to_fwd(dir.join("logs"));
    let sites  = to_fwd(dir.join("sites"));
    let _ = https_port;

    let conf = format!(
        r#"worker_processes 1;
error_log  "{logs}/error.log";
pid        "{logs}/nginx.pid";

events {{
    worker_connections 1024;
}}

http {{
    include       "{mime}";
    default_type  application/octet-stream;

    access_log  "{logs}/access.log";

    sendfile        on;
    keepalive_timeout  65;

    server {{
        listen       {http_port};
        server_name  localhost;
        return       444;
    }}

    include "{sites}/*.conf";
}}
"#
    );

    std::fs::write(conf_path, conf).map_err(|e| e.to_string())?;
    Ok(())
}

const BASIC_MIME_TYPES: &str = r#"types {
    text/html                             html htm shtml;
    text/css                              css;
    text/xml                              xml;
    application/javascript                js;
    application/json                      json;
    image/gif                             gif;
    image/jpeg                            jpeg jpg;
    image/png                             png;
    image/svg+xml                         svg svgz;
    image/webp                            webp;
    image/x-icon                          ico;
    font/woff                             woff;
    font/woff2                            woff2;
    application/octet-stream              bin exe dll;
    application/zip                       zip;
    application/pdf                       pdf;
}
"#;
