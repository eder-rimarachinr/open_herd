use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use parking_lot::Mutex;
use crate::domain::ports::logger::LoggerPort;
use crate::infrastructure::{state::AppState, nginx::vhost_config, process_guard};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// The nginx master we spawned, plus what we learned about it at start.
struct RunningNginx {
    child: Child,
    version: Option<String>,
    pid: u32,
}

/// Point-in-time view of the tracked nginx process.
pub struct NginxSnapshot {
    pub running: bool,
    pub version: Option<String>,
    pub pid: Option<u32>,
}

pub struct NginxProcess {
    /// Serialises start / stop / reload. Held for the whole operation (which
    /// spawns processes and sleeps), so it must never be needed to read status.
    op_lock: Mutex<()>,
    /// The tracked master. Only ever locked briefly.
    running: Mutex<Option<RunningNginx>>,
}

impl NginxProcess {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { op_lock: Mutex::new(()), running: Mutex::new(None) })
    }

    /// Cheap and non-blocking: safe to call from async code. Forgets a master
    /// that has exited so status never reports a dead process.
    pub fn snapshot(&self) -> NginxSnapshot {
        let mut running = self.running.lock();
        if let Some(r) = running.as_mut() {
            if matches!(r.child.try_wait(), Ok(None)) {
                return NginxSnapshot { running: true, version: r.version.clone(), pid: Some(r.pid) };
            }
            *running = None;
        }
        NginxSnapshot { running: false, version: None, pid: None }
    }

    pub fn is_running(&self) -> bool {
        self.snapshot().running
    }
}

pub fn find_nginx_binary(nginx_dir: &str) -> Option<PathBuf> {
    let win = Path::new(nginx_dir).join("nginx.exe");
    // A half-extracted download must not count as installed.
    if win.exists() && crate::infrastructure::download::is_install_complete(Path::new(nginx_dir)) { return Some(win); }
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

/// En Windows cada arranque de nginx crea un proceso totalmente independiente
/// (no hay fork real como en Unix), y `nginx -s reload`/`-s quit` dependen de
/// un named event atado al PID que aparece en `nginx.pid`. Si el daemon se
/// reinicia pierde el `Child` de la sesión anterior, y si dos arranques
/// llegan a solaparse ese archivo puede quedar apuntando a un PID muerto —
/// entonces las señales se pierden en el vacío y el nginx huérfano sigue
/// vivo, sirviendo una config vieja en los puertos 80/443. Por eso, antes de
/// levantar un proceso nuevo (o al detenerlo) buscamos y matamos por ruta de
/// ejecutable exacta cualquier `nginx.exe` que sea el nuestro, sin depender
/// del pid file ni del `Child` en memoria.
#[cfg(target_os = "windows")]
fn kill_stale_nginx(binary: &Path) {
    let binary_str = binary.to_string_lossy().replace('\'', "''");
    let script = format!(
        "Get-CimInstance Win32_Process -Filter \"Name='nginx.exe'\" | \
         Where-Object {{ $_.ExecutablePath -eq '{}' }} | \
         ForEach-Object {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }}",
        binary_str
    );
    #[allow(unused_mut)]
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
    cmd.creation_flags(CREATE_NO_WINDOW);
    let _ = cmd.output();
}

#[cfg(not(target_os = "windows"))]
fn kill_stale_nginx(_binary: &Path) {}

pub fn get_nginx_version(binary: &Path) -> Option<String> {
    #[allow(unused_mut)]
    let mut cmd = Command::new(binary);
    cmd.arg("-v");
    #[cfg(target_os = "windows")] cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().ok()?;
    let line = String::from_utf8_lossy(&out.stderr).lines().find(|l| l.contains("nginx/"))?.to_string();
    Some(line.split('/').nth(1)?.trim().to_string())
}

/// Blocking (spawns processes, sleeps): call from a blocking thread.
pub fn start(state: &AppState, nginx_proc: &NginxProcess, logger: &dyn LoggerPort) -> Result<(), String> {
    let _op = nginx_proc.op_lock.lock();
    if nginx_proc.is_running() { return Err("nginx is already running".into()); }

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
            let ports = vhost_config::ListenPorts { http: http_port, https: https_port };
            let _ = vhost_config::generate_with_certs(site, &nginx_dir, ports, Some(&certs_dir));
        }
    }

    ensure_config(&nginx_dir, &binary, http_port, https_port)?;
    let conf_path = PathBuf::from(&nginx_dir).join("nginx.conf");
    test_config(&binary, &conf_path)?;

    // No hay Child rastreado y vivo — puede que un nginx huérfano de una
    // sesión anterior siga ocupando los puertos con una config vieja.
    kill_stale_nginx(&binary);

    let bin_dir   = binary.parent().unwrap_or(&binary);
    let error_log = PathBuf::from(&nginx_dir).join("logs").join("error.log");
    let mut cmd = Command::new(&binary);
    cmd.current_dir(bin_dir)
        .args(["-c", &conf_path.to_string_lossy(), "-e", &error_log.to_string_lossy()]);
    let mut child = process_guard::spawn_guarded(&mut cmd, logger)
        .map_err(|e| format!("Failed to spawn nginx: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(300));
    if let Ok(Some(status)) = child.try_wait() {
        return Err(format!("nginx exited immediately (code {:?})", status.code()));
    }
    let version = get_nginx_version(&binary);
    let pid     = child.id();
    *nginx_proc.running.lock() = Some(RunningNginx { child, version, pid });
    logger.log("Nginx started".into());
    Ok(())
}

/// Blocking (spawns processes, sleeps): call from a blocking thread.
pub fn stop(state: &AppState, nginx_proc: &NginxProcess, logger: &dyn LoggerPort) -> Result<(), String> {
    let _op = nginx_proc.op_lock.lock();
    let binary = { let cfg = state.config.read(); find_nginx_binary(&cfg.nginx_dir) };
    if let Some(bin) = &binary {
        #[allow(unused_mut)]
        let mut cmd = Command::new(bin);
        cmd.args(["-s", "quit"]);
        #[cfg(target_os = "windows")] cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = cmd.output();
    }
    // Take it out first so status readers see "stopped" without waiting for
    // the grace period below.
    let tracked = nginx_proc.running.lock().take();
    if let Some(mut r) = tracked {
        std::thread::sleep(std::time::Duration::from_millis(500));
        let _ = r.child.kill(); let _ = r.child.wait();
    }
    // `-s quit` puede fallar en silencio si nginx.pid está desincronizado
    // (ver comentario en kill_stale_nginx) — esto garantiza que no quede
    // nada nuestro corriendo, sin depender de esa señal.
    if let Some(bin) = &binary { kill_stale_nginx(bin); }
    logger.log("Nginx stopped".into());
    Ok(())
}

/// No-op when nginx is not running: on Windows `nginx -s reload` signals a
/// named event tied to the master's PID and fails with "OpenEvent failed" when
/// there is none. Blocking (spawns a process): call from a blocking thread.
pub fn reload(state: &AppState, nginx_proc: &NginxProcess, logger: &dyn LoggerPort) -> Result<(), String> {
    let _op = nginx_proc.op_lock.lock();
    if !nginx_proc.is_running() { return Ok(()); }
    let (binary, conf_path) = { let cfg = state.config.read(); (find_nginx_binary(&cfg.nginx_dir), PathBuf::from(&cfg.nginx_dir).join("nginx.conf")) };
    let binary = binary.ok_or_else(|| "nginx binary not found".to_string())?;
    let bin_dir = binary.parent().unwrap_or(&binary);
    #[allow(unused_mut)]
    let mut cmd = Command::new(&binary);
    cmd.current_dir(bin_dir).args(["-s", "reload", "-c", &conf_path.to_string_lossy()]);
    #[cfg(target_os = "windows")] cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() { logger.log("Nginx reloaded".into()); Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).to_string()) }
}

fn test_binary(binary: &Path) -> Result<(), String> {
    #[allow(unused_mut)]
    let mut cmd = Command::new(binary);
    cmd.arg("-v");
    #[cfg(target_os = "windows")] cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().map_err(|e| format!("Cannot run nginx: {}", e))?;
    if !out.status.success() { return Err(format!("nginx binary error: {}", String::from_utf8_lossy(&out.stderr).trim())); }
    Ok(())
}

fn test_config(binary: &Path, conf: &Path) -> Result<(), String> {
    let bin_dir = binary.parent().unwrap_or(binary);
    #[allow(unused_mut)]
    let mut cmd = Command::new(binary);
    cmd.current_dir(bin_dir).args(["-t", "-c", &conf.to_string_lossy()]);
    #[cfg(target_os = "windows")] cmd.creation_flags(CREATE_NO_WINDOW);
    let out = cmd.output().map_err(|e| e.to_string())?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_process_reports_not_running_with_no_version_or_pid() {
        let proc = NginxProcess::new();
        let snap = proc.snapshot();
        assert!(!snap.running);
        assert!(snap.version.is_none());
        assert!(snap.pid.is_none());
    }
}
