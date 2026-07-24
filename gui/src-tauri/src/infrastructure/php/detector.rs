use async_trait::async_trait;
use std::process::Command;
use crate::domain::ports::process_manager::{PhpDetectorPort, PhpInstallation};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct SystemPhpDetector;
impl SystemPhpDetector { pub fn new() -> Self { Self } }

#[async_trait]
impl PhpDetectorPort for SystemPhpDetector {
    async fn detect(&self) -> Vec<PhpInstallation> {
        tokio::task::spawn_blocking(find_php_binaries).await.unwrap_or_default()
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
        #[allow(unused_mut)] let mut cmd = Command::new(&binary_path); cmd.arg("--version");
        #[cfg(target_os = "windows")] cmd.creation_flags(CREATE_NO_WINDOW);
        if let Ok(out) = cmd.output() {
            if let Some(ver) = parse_php_version(&String::from_utf8_lossy(&out.stdout)) {
                let major = ver.split('.').take(2).collect::<Vec<_>>().join(".");
                if !found.iter().any(|f| f.major == major) { found.push(PhpInstallation { major, version: ver, binary_path }); }
            }
        }
    }
    #[allow(unused_mut)] let mut cmd2 = Command::new("php"); cmd2.arg("--version");
    #[cfg(target_os = "windows")] cmd2.creation_flags(CREATE_NO_WINDOW);
    if let Ok(out) = cmd2.output() {
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
