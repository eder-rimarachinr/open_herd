use std::collections::HashMap;
use std::process::{Child, Command};
use std::sync::Arc;
use parking_lot::Mutex;

use super::models::PhpVersion;
use super::site_config::fastcgi_port;
use super::state::AppState;

pub struct PhpProcesses {
    /// major version (e.g. "8.2") → running php-cgi child
    pub children: Mutex<HashMap<String, Child>>,
}

impl PhpProcesses {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { children: Mutex::new(HashMap::new()) })
    }
}

/// Start php-cgi for a given PHP version on its designated port.
pub fn start(state: &Arc<AppState>, php_proc: &Arc<PhpProcesses>, version: &PhpVersion) -> Result<(), String> {
    let port = fastcgi_port(&version.major);
    let mut children = php_proc.children.lock();

    if children.contains_key(&version.major) {
        return Ok(()); // already running
    }

    // On Windows use php-cgi.exe; on Linux use php-fpm
    #[cfg(target_os = "windows")]
    let binary = find_php_cgi_windows(&version.binary_path)?;
    #[cfg(not(target_os = "windows"))]
    let binary = version.fpm_binary.clone();

    let child = Command::new(&binary)
        .args(["-b", &format!("127.0.0.1:{}", port)])
        .spawn()
        .map_err(|e| format!("Failed to start PHP {}: {}", version.major, e))?;

    state.log(format!("PHP {} started on port {}", version.major, port));
    children.insert(version.major.clone(), child);
    Ok(())
}

pub fn stop(state: &Arc<AppState>, php_proc: &Arc<PhpProcesses>, major: &str) -> Result<(), String> {
    let mut children = php_proc.children.lock();
    if let Some(mut child) = children.remove(major) {
        let _ = child.kill();
        let _ = child.wait();
        state.log(format!("PHP {} stopped", major));
    }
    Ok(())
}

pub fn stop_all(state: &Arc<AppState>, php_proc: &Arc<PhpProcesses>) {
    let mut children = php_proc.children.lock();
    for (major, mut child) in children.drain() {
        let _ = child.kill();
        let _ = child.wait();
        state.log(format!("PHP {} stopped", major));
    }
}

pub fn is_running(php_proc: &Arc<PhpProcesses>, major: &str) -> bool {
    let mut children = php_proc.children.lock();
    if let Some(child) = children.get_mut(major) {
        match child.try_wait() {
            Ok(None) => true,
            _ => { children.remove(major); false }
        }
    } else {
        false
    }
}

pub fn running_versions(php_proc: &Arc<PhpProcesses>) -> Vec<String> {
    let mut children = php_proc.children.lock();
    let mut alive = vec![];
    let mut dead = vec![];
    for (major, child) in children.iter_mut() {
        match child.try_wait() {
            Ok(None) => alive.push(major.clone()),
            _ => dead.push(major.clone()),
        }
    }
    for d in dead { children.remove(&d); }
    alive
}

#[cfg(target_os = "windows")]
fn find_php_cgi_windows(binary_path: &str) -> Result<String, String> {
    // binary_path is php.exe — look for php-cgi.exe in the same dir
    let p = std::path::Path::new(binary_path);
    let dir = p.parent().unwrap_or(p);
    let cgi = dir.join("php-cgi.exe");
    if cgi.exists() {
        return Ok(cgi.to_string_lossy().into());
    }
    // fallback: try php-cgi in PATH
    which::which("php-cgi")
        .map(|p| p.to_string_lossy().into())
        .map_err(|_| format!("php-cgi.exe not found near {}", binary_path))
}
