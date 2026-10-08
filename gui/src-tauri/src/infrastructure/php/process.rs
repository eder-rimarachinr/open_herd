use std::collections::HashMap;
use std::process::{Child, Command};
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use crate::domain::ports::logger::LoggerPort;
use crate::infrastructure::{dto::PhpVersion, nginx::vhost_config::fastcgi_port, process_guard};

/// A supervised php-cgi process and what is needed to start it again.
struct PhpChild {
    child: Child,
    binary: String,
    port: u16,
    /// When the supervisor restarted it, for crash-loop detection.
    restarts: Vec<Instant>,
}

/// Restarts allowed within `RESTART_WINDOW` before the supervisor gives up on a
/// version that keeps dying (bad php.ini, port taken, missing DLL…).
const MAX_RESTARTS: usize = 5;
const RESTART_WINDOW: Duration = Duration::from_secs(60);

pub struct PhpProcesses { children: Mutex<HashMap<String, PhpChild>> }
impl PhpProcesses { pub fn new() -> Arc<Self> { Arc::new(Self { children: Mutex::new(HashMap::new()) }) } }

fn php_cgi_command(binary: &str, port: u16) -> Command {
    let mut cmd = Command::new(binary);
    cmd.args(["-b", &format!("127.0.0.1:{}", port)])
        // php-cgi exits after 500 requests by default; nothing else would
        // restart it and every site on that version would answer 502. 0 = never.
        .env("PHP_FCGI_MAX_REQUESTS", "0");
    cmd
}

fn spawn_php_cgi(binary: &str, port: u16, logger: &dyn LoggerPort) -> std::io::Result<Child> {
    process_guard::spawn_guarded(&mut php_cgi_command(binary, port), logger)
}

pub fn start(php_proc: &PhpProcesses, logger: &dyn LoggerPort, version: &PhpVersion) -> Result<(), String> {
    let port     = fastcgi_port(&version.major);
    let mut children = php_proc.children.lock();
    if children.get_mut(&version.major).is_some_and(|c| matches!(c.child.try_wait(), Ok(None))) {
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    let binary = find_php_cgi_windows(&version.binary_path)?;
    #[cfg(not(target_os = "windows"))]
    let binary = version.fpm_binary.clone();

    let child = spawn_php_cgi(&binary, port, logger)
        .map_err(|e| format!("Failed to start PHP {}: {}", version.major, e))?;
    logger.log(format!("PHP {} started on port {}", version.major, port));
    children.insert(version.major.clone(), PhpChild { child, binary, port, restarts: Vec::new() });
    Ok(())
}

/// Blocking (waits for the process to exit): call from a blocking thread.
pub fn stop(php_proc: &PhpProcesses, logger: &dyn LoggerPort, major: &str) -> Result<(), String> {
    // Remove first, so the supervisor never restarts a version being stopped.
    let removed = php_proc.children.lock().remove(major);
    if let Some(mut c) = removed {
        let _ = c.child.kill(); let _ = c.child.wait();
        logger.log(format!("PHP {} stopped", major));
    }
    Ok(())
}

/// Blocking (waits for the processes to exit): call from a blocking thread.
pub fn stop_all(php_proc: &PhpProcesses, logger: &dyn LoggerPort) {
    let drained: Vec<_> = php_proc.children.lock().drain().collect();
    for (major, mut c) in drained {
        let _ = c.child.kill(); let _ = c.child.wait();
        logger.log(format!("PHP {} stopped", major));
    }
}

pub fn is_running(php_proc: &PhpProcesses, major: &str) -> bool {
    php_proc.children.lock().get_mut(major).is_some_and(|c| matches!(c.child.try_wait(), Ok(None)))
}

/// Versions whose process is alive right now. A version that just died stays
/// tracked (but is not listed) until the supervisor restarts or drops it.
pub fn running_versions(php_proc: &PhpProcesses) -> Vec<String> {
    php_proc.children.lock().iter_mut()
        .filter_map(|(major, c)| matches!(c.child.try_wait(), Ok(None)).then(|| major.clone()))
        .collect()
}

/// Restarts php-cgi processes that exited without being stopped through us,
/// giving up on a version after `MAX_RESTARTS` within `RESTART_WINDOW`.
/// Called periodically by the container's background task.
pub fn supervise(php_proc: &PhpProcesses, logger: &dyn LoggerPort) {
    let mut children = php_proc.children.lock();
    let now = Instant::now();
    let mut dropped = Vec::new();
    for (major, c) in children.iter_mut() {
        let status = match c.child.try_wait() {
            Ok(None) => continue,
            Ok(Some(status)) => status.to_string(),
            Err(e) => e.to_string(),
        };
        if !allow_restart(&mut c.restarts, now) {
            logger.log(format!(
                "PHP {major} keeps exiting ({status}); stopped restarting it. Check its php.ini / error log and start it again."
            ));
            dropped.push(major.clone());
            continue;
        }
        match spawn_php_cgi(&c.binary, c.port, logger) {
            Ok(child) => {
                c.child = child;
                logger.log(format!("PHP {major} exited unexpectedly ({status}); restarted"));
            }
            Err(e) => {
                logger.log(format!("PHP {major} exited unexpectedly ({status}) and could not be restarted: {e}"));
                dropped.push(major.clone());
            }
        }
    }
    for major in dropped { children.remove(&major); }
}

/// Records a restart at `now` unless `MAX_RESTARTS` already happened within
/// `RESTART_WINDOW`.
fn allow_restart(history: &mut Vec<Instant>, now: Instant) -> bool {
    history.retain(|t| now.duration_since(*t) < RESTART_WINDOW);
    if history.len() >= MAX_RESTARTS { return false; }
    history.push(now);
    true
}

#[cfg(target_os = "windows")]
fn find_php_cgi_windows(binary_path: &str) -> Result<String, String> {
    let p   = std::path::Path::new(binary_path);
    let dir = p.parent().unwrap_or(p);
    let cgi = dir.join("php-cgi.exe");
    if cgi.exists() { return Ok(cgi.to_string_lossy().into()); }
    which::which("php-cgi").map(|p| p.to_string_lossy().into()).map_err(|_| format!("php-cgi.exe not found near {}", binary_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::logging::InMemoryLogger;

    #[test]
    fn php_cgi_never_recycles_itself() {
        let cmd = php_cgi_command("php-cgi", 9082);
        let max = cmd.get_envs().find(|(k, _)| *k == "PHP_FCGI_MAX_REQUESTS").and_then(|(_, v)| v);
        assert_eq!(max, Some(std::ffi::OsStr::new("0")));
    }

    #[test]
    fn restart_budget_resets_after_the_window() {
        let t0 = Instant::now();
        let mut history = Vec::new();
        for _ in 0..MAX_RESTARTS { assert!(allow_restart(&mut history, t0)); }
        assert!(!allow_restart(&mut history, t0 + Duration::from_secs(1)));
        assert!(allow_restart(&mut history, t0 + RESTART_WINDOW + Duration::from_secs(1)));
    }

    fn wait_until_exited(procs: &PhpProcesses, major: &str) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let exited = procs.children.lock().get_mut(major)
                .is_none_or(|c| !matches!(c.child.try_wait(), Ok(None)));
            if exited { return; }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("child did not exit in time");
    }

    /// Uses this test binary as a stand-in php-cgi: it rejects `-b` and exits
    /// at once, i.e. a process that crashes on every start.
    #[test]
    fn crash_looping_version_is_restarted_then_dropped() {
        let exe = std::env::current_exe().unwrap().to_string_lossy().into_owned();
        let logger = InMemoryLogger::new();
        let procs = PhpProcesses::new();
        let child = spawn_php_cgi(&exe, 1, logger.as_ref()).unwrap();
        procs.children.lock().insert("9.9".into(), PhpChild { child, binary: exe, port: 1, restarts: Vec::new() });

        for _ in 0..=MAX_RESTARTS {
            wait_until_exited(&procs, "9.9");
            supervise(&procs, logger.as_ref());
        }

        assert!(!procs.children.lock().contains_key("9.9"));
        let log = logger.recent(100).join("\n");
        assert_eq!(log.matches("restarted").count(), MAX_RESTARTS);
        assert!(log.contains("stopped restarting"));
    }

    #[test]
    fn stopped_version_is_not_restarted() {
        let exe = std::env::current_exe().unwrap().to_string_lossy().into_owned();
        let logger = InMemoryLogger::new();
        let procs = PhpProcesses::new();
        let child = spawn_php_cgi(&exe, 1, logger.as_ref()).unwrap();
        procs.children.lock().insert("9.9".into(), PhpChild { child, binary: exe, port: 1, restarts: Vec::new() });

        stop(&procs, logger.as_ref(), "9.9").unwrap();
        supervise(&procs, logger.as_ref());

        assert!(!procs.children.lock().contains_key("9.9"));
        assert!(!logger.recent(100).join("\n").contains("restarted"));
    }
}
