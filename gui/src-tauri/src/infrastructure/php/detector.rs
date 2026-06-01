use async_trait::async_trait;
use std::sync::Arc;
use crate::infrastructure::{dto, state::AppState};
use crate::domain::ports::process_manager::{PhpDetectorPort, PhpInstallation};

pub struct SystemPhpDetector { state: Arc<AppState> }
impl SystemPhpDetector { pub fn new(state: Arc<AppState>) -> Self { Self { state } } }

#[async_trait]
impl PhpDetectorPort for SystemPhpDetector {
    async fn detect(&self) -> Vec<PhpInstallation> {
        let state = self.state.clone();
        tokio::task::spawn_blocking(move || {
            let installs = find_php_binaries();
            let legacy: Vec<dto::PhpVersion> = installs.iter().map(|p| {
                let parts: Vec<u16> = p.major.split('.').filter_map(|s| s.parse().ok()).collect();
                let port = match parts.as_slice() { [maj, min, ..] => 9000 + maj * 10 + min, [maj] => 9000 + maj * 10, _ => 9082 };
                dto::PhpVersion { version: p.version.clone(), major: p.major.clone(), binary_path: p.binary_path.clone(), fpm_binary: p.binary_path.clone(), fastcgi_addr: format!("127.0.0.1:{}", port), installed: true, running: false }
            }).collect();
            *state.php_versions.write() = legacy;
            installs
        }).await.unwrap_or_default()
    }
}

fn find_php_binaries() -> Vec<PhpInstallation> {
    let mut found: Vec<PhpInstallation> = Vec::new();
    let mut search_dirs: Vec<std::path::PathBuf> = vec![];

    if let Some(home) = dirs_next::home_dir() {
        let app_php = home.join(".phpenv").join("php");
        if app_php.exists() { if let Ok(entries) = std::fs::read_dir(&app_php) { for e in entries.flatten() { if e.path().is_dir() { search_dirs.push(e.path()); } } } }
    }
    #[cfg(target_os = "windows")] {
        for dir in &["C:/xampp/php","C:/wamp64/bin/php","C:/laragon/bin/php"] { search_dirs.push(std::path::PathBuf::from(dir)); }
        let wamp = std::path::Path::new("C:/wamp64/bin/php");
        if wamp.exists() { if let Ok(entries) = std::fs::read_dir(wamp) { for e in entries.flatten() { if e.path().is_dir() { search_dirs.push(e.path()); } } } }
    }
    #[cfg(not(target_os = "windows"))] {
        for dir in &["/usr/bin","/usr/local/bin"] { search_dirs.push(std::path::PathBuf::from(dir)); }
    }

    let binary_name = if cfg!(target_os = "windows") { "php.exe" } else { "php" };
    for dir in &search_dirs {
        let bin = dir.join(binary_name);
        if !bin.exists() { continue; }
        let binary_path = bin.to_string_lossy().to_string();
        if let Ok(out) = std::process::Command::new(&binary_path).arg("--version").output() {
            if let Some(ver) = parse_php_version(&String::from_utf8_lossy(&out.stdout)) {
                let major = ver.split('.').take(2).collect::<Vec<_>>().join(".");
                if !found.iter().any(|f| f.major == major) { found.push(PhpInstallation { major, version: ver, binary_path }); }
            }
        }
    }
    if let Ok(out) = std::process::Command::new("php").arg("--version").output() {
        if let Some(ver) = parse_php_version(&String::from_utf8_lossy(&out.stdout)) {
            let major = ver.split('.').take(2).collect::<Vec<_>>().join(".");
            if !found.iter().any(|f| f.major == major) { found.push(PhpInstallation { major, version: ver, binary_path: "php".into() }); }
        }
    }
    found
}

fn parse_php_version(output: &str) -> Option<String> {
    let line = output.lines().next()?;
    if !line.starts_with("PHP ") { return None; }
    Some(line.split_whitespace().nth(1)?.to_string())
}
