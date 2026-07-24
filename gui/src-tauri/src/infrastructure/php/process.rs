use std::collections::HashMap;
use std::process::{Child, Command};
use std::sync::Arc;
use parking_lot::Mutex;
use crate::domain::ports::logger::LoggerPort;
use crate::infrastructure::{dto::PhpVersion, nginx::vhost_config::fastcgi_port, state::AppState};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct PhpProcesses { pub children: Mutex<HashMap<String, Child>> }
impl PhpProcesses { pub fn new() -> Arc<Self> { Arc::new(Self { children: Mutex::new(HashMap::new()) }) } }

// `state` ya no se usa en el cuerpo (solo se usaba para `.log(...)`, ahora vía `logger`),
// pero se conserva en la firma por simetría con `nginx::process` y para no romper a los
// call sites, que siguen pasando `&AppState` desde `PhpProcessAdapter`.
pub fn start(_state: &AppState, php_proc: &Arc<PhpProcesses>, logger: &Arc<dyn LoggerPort>, version: &PhpVersion) -> Result<(), String> {
    let port     = fastcgi_port(&version.major);
    let mut children = php_proc.children.lock();
    if children.contains_key(&version.major) { return Ok(()); }

    #[cfg(target_os = "windows")]
    let binary = find_php_cgi_windows(&version.binary_path)?;
    #[cfg(not(target_os = "windows"))]
    let binary = version.fpm_binary.clone();

    #[allow(unused_mut)]
    let mut cmd = Command::new(&binary);
    cmd.args(["-b", &format!("127.0.0.1:{}", port)]);
    #[cfg(target_os = "windows")] cmd.creation_flags(CREATE_NO_WINDOW);
    let child = cmd.spawn()
        .map_err(|e| format!("Failed to start PHP {}: {}", version.major, e))?;
    logger.log(format!("PHP {} started on port {}", version.major, port));
    children.insert(version.major.clone(), child);
    Ok(())
}

pub fn stop(_state: &AppState, php_proc: &Arc<PhpProcesses>, logger: &Arc<dyn LoggerPort>, major: &str) -> Result<(), String> {
    let mut children = php_proc.children.lock();
    if let Some(mut child) = children.remove(major) { let _ = child.kill(); let _ = child.wait(); logger.log(format!("PHP {} stopped", major)); }
    Ok(())
}

pub fn stop_all(_state: &AppState, php_proc: &Arc<PhpProcesses>, logger: &Arc<dyn LoggerPort>) {
    let mut children = php_proc.children.lock();
    for (major, mut child) in children.drain() { let _ = child.kill(); let _ = child.wait(); logger.log(format!("PHP {} stopped", major)); }
}

pub fn is_running(php_proc: &Arc<PhpProcesses>, major: &str) -> bool {
    let mut children = php_proc.children.lock();
    if let Some(child) = children.get_mut(major) {
        match child.try_wait() { Ok(None) => true, _ => { children.remove(major); false } }
    } else { false }
}

pub fn running_versions(php_proc: &Arc<PhpProcesses>) -> Vec<String> {
    let mut children = php_proc.children.lock();
    let mut alive = vec![]; let mut dead = vec![];
    for (major, child) in children.iter_mut() { match child.try_wait() { Ok(None) => alive.push(major.clone()), _ => dead.push(major.clone()) } }
    for d in dead { children.remove(&d); }
    alive
}

#[cfg(target_os = "windows")]
fn find_php_cgi_windows(binary_path: &str) -> Result<String, String> {
    let p   = std::path::Path::new(binary_path);
    let dir = p.parent().unwrap_or(p);
    let cgi = dir.join("php-cgi.exe");
    if cgi.exists() { return Ok(cgi.to_string_lossy().into()); }
    which::which("php-cgi").map(|p| p.to_string_lossy().into()).map_err(|_| format!("php-cgi.exe not found near {}", binary_path))
}
