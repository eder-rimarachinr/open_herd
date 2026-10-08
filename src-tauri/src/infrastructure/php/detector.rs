use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use crate::domain::ports::process_manager::{PhpDetectorPort, PhpInstallation};
use crate::infrastructure::{download, state::AppState};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Reads `php_dir` / `custom_php_dirs` from the live config on every
/// detection, so portable mode and Settings changes are honoured.
pub struct SystemPhpDetector { state: Arc<AppState> }

impl SystemPhpDetector { pub fn new(state: Arc<AppState>) -> Self { Self { state } } }

#[async_trait]
impl PhpDetectorPort for SystemPhpDetector {
    async fn detect(&self) -> Vec<PhpInstallation> {
        let dirs = {
            let cfg = self.state.config.read();
            search_dirs(Path::new(&cfg.php_dir), &cfg.custom_php_dirs)
        };
        tokio::task::spawn_blocking(move || find_php_binaries(&dirs)).await.unwrap_or_default()
    }
}

/// Directories that may directly contain a PHP binary, in priority order:
/// our own installs (`php_dir/<major>`), then user-configured dirs, then
/// well-known third-party stacks. Each root also contributes its immediate
/// subdirectories (`php_dir/8.2`, `wamp64/bin/php/php8.2.0`, …).
fn search_dirs(php_dir: &Path, custom_dirs: &[String]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = vec![php_dir.to_path_buf()];
    roots.extend(custom_dirs.iter().map(PathBuf::from));
    #[cfg(target_os = "windows")]
    roots.extend(["C:/xampp/php", "C:/wamp64/bin/php", "C:/laragon/bin/php"].map(PathBuf::from));
    #[cfg(not(target_os = "windows"))]
    roots.extend(["/usr/bin", "/usr/local/bin"].map(PathBuf::from));

    let mut dirs = Vec::new();
    for root in roots {
        let children = std::fs::read_dir(&root).into_iter().flatten().flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir() && download::is_install_complete(p));
        let mut children: Vec<PathBuf> = children.collect();
        children.sort();
        if download::is_install_complete(&root) { dirs.push(root); }
        dirs.extend(children);
    }
    dirs
}

fn find_php_binaries(search_dirs: &[PathBuf]) -> Vec<PhpInstallation> {
    let mut found: Vec<PhpInstallation> = Vec::new();

    let binary_name = if cfg!(target_os = "windows") { "php.exe" } else { "php" };
    for dir in search_dirs {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn searches_configured_php_dir_and_custom_dirs_with_their_versions() {
        let tmp = TempDir::new().unwrap();
        let php_dir = tmp.path().join("portable-data").join("php");
        std::fs::create_dir_all(php_dir.join("8.2")).unwrap();
        std::fs::create_dir_all(php_dir.join("8.3")).unwrap();
        let custom = tmp.path().join("my-php");
        std::fs::create_dir_all(&custom).unwrap();

        let dirs = search_dirs(&php_dir, &[custom.to_string_lossy().into_owned()]);

        assert!(dirs.contains(&php_dir.join("8.2")));
        assert!(dirs.contains(&php_dir.join("8.3")));
        assert!(dirs.contains(&custom));
        let pos = |p: &Path| dirs.iter().position(|d| d == p).unwrap();
        assert!(pos(&php_dir.join("8.2")) < pos(&custom), "own installs come first");
    }

    #[test]
    fn half_extracted_install_is_ignored() {
        let tmp = TempDir::new().unwrap();
        let php_dir = tmp.path().join("php");
        std::fs::create_dir_all(php_dir.join("8.4")).unwrap();
        std::fs::write(php_dir.join("8.4").join(download::INSTALL_INCOMPLETE_MARKER), "").unwrap();

        assert!(!search_dirs(&php_dir, &[]).contains(&php_dir.join("8.4")));
    }
}
