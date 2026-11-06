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

#[cfg(windows)]
fn php_cgi_command(binary: &str, port: u16) -> Command {
    let mut cmd = Command::new(binary);
    cmd.args(["-b", &format!("127.0.0.1:{}", port)])
        // php-cgi exits after 500 requests by default; nothing else would
        // restart it and every site on that version would answer 502. 0 = never.
        .env("PHP_FCGI_MAX_REQUESTS", "0");
    cmd
}

/// The FastCGI server for one PHP version: `php-cgi -b` on Windows.
#[cfg(windows)]
fn fastcgi_command(binary: &str, port: u16) -> std::io::Result<Command> {
    Ok(php_cgi_command(binary, port))
}

/// php-fpm pool config: one pool on the TCP port php-cgi uses on Windows, so
/// the nginx vhosts are the same on both systems.
#[cfg(not(windows))]
fn fpm_config(port: u16, error_log: &std::path::Path) -> String {
    format!("\
[global]
error_log = {}
daemonize = no

[www]
listen = 127.0.0.1:{port}
pm = dynamic
pm.max_children = 8
pm.start_servers = 2
pm.min_spare_servers = 1
pm.max_spare_servers = 3
clear_env = no
catch_workers_output = yes
", error_log.display())
}

/// The FastCGI server for one PHP version on Linux: php-fpm in the foreground
/// (so it stays our child and the supervisor sees it exit) with a generated
/// pool config, and the install's own php.ini when it has one.
#[cfg(not(windows))]
fn fastcgi_command(binary: &str, port: u16) -> std::io::Result<Command> {
    let dir = std::env::temp_dir().join("open-herd");
    std::fs::create_dir_all(&dir)?;
    let conf = dir.join(format!("php-fpm-{port}.conf"));
    std::fs::write(&conf, fpm_config(port, &dir.join(format!("php-fpm-{port}.log"))))?;
    let mut cmd = Command::new(binary);
    cmd.arg("--nodaemonize").arg("--fpm-config").arg(&conf);
    let ini = std::path::Path::new(binary).with_file_name("php.ini");
    if ini.is_file() { cmd.arg("-c").arg(ini); }
    Ok(cmd)
}

fn spawn_php_cgi(binary: &str, port: u16, logger: &dyn LoggerPort) -> std::io::Result<Child> {
    process_guard::spawn_guarded(&mut fastcgi_command(binary, port)?, logger)
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
    let binary = find_php_fpm(&version.fpm_binary, &version.major)?;

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

/// php-fpm for the PHP whose CLI is `binary_path`: beside it (Open Herd's
/// static builds), Debian/Ubuntu's `/usr/sbin/php-fpm<major>`, then the PATH.
#[cfg(not(target_os = "windows"))]
fn find_php_fpm(binary_path: &str, major: &str) -> Result<String, String> {
    let cli = std::path::Path::new(binary_path);
    let mut candidates = Vec::new();
    if cli.is_absolute() { candidates.push(cli.with_file_name("php-fpm")); }
    candidates.push(std::path::PathBuf::from(format!("/usr/sbin/php-fpm{major}")));
    if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
        return Ok(found.to_string_lossy().into());
    }
    which::which(format!("php-fpm{major}")).or_else(|_| which::which("php-fpm"))
        .map(|p| p.to_string_lossy().into())
        .map_err(|_| format!("php-fpm not found for PHP {major} (looked beside {binary_path}, in /usr/sbin and on the PATH)"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::logging::InMemoryLogger;

    #[test]
    #[cfg(windows)]
    fn php_cgi_never_recycles_itself() {
        let cmd = php_cgi_command("php-cgi", 9082);
        let max = cmd.get_envs().find(|(k, _)| *k == "PHP_FCGI_MAX_REQUESTS").and_then(|(_, v)| v);
        assert_eq!(max, Some(std::ffi::OsStr::new("0")));
    }

    #[test]
    #[cfg(not(windows))]
    fn fpm_listens_on_the_version_port_in_the_foreground() {
        let conf = fpm_config(9084, std::path::Path::new("/tmp/fpm.log"));
        assert!(conf.contains("listen = 127.0.0.1:9084\n"), "{conf}");
        assert!(conf.contains("daemonize = no"));
        assert!(conf.contains("error_log = /tmp/fpm.log"));
        let cmd = fastcgi_command("/opt/php/8.4/php-fpm", 9084).unwrap();
        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args[0], "--nodaemonize");
        assert!(args[2].ends_with("php-fpm-9084.conf"), "{args:?}");
    }

    #[test]
    #[cfg(not(windows))]
    fn php_fpm_is_found_beside_the_cli() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("php"), "").unwrap();
        std::fs::write(tmp.path().join("php-fpm"), "").unwrap();
        let cli = tmp.path().join("php").to_string_lossy().into_owned();
        assert_eq!(find_php_fpm(&cli, "8.4").unwrap(), tmp.path().join("php-fpm").to_string_lossy());
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
