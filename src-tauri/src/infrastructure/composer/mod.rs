//! Composer adapters: the `composer` command (a `.bat` / `sh` shim that runs
//! `composer.phar` with the default PHP), and getcomposer.org's release list.
//!
//! Layout under `base_dir`:
//! - `composer/composer.phar` and `composer/open-herd.json` (its version)
//! - `bin/composer.bat` (Windows, `bin` on the user PATH) or `bin/composer`
//!   (Linux, linked from `~/.local/bin`)

#[cfg(not(windows))]
pub mod linux;
pub mod releases;
#[cfg(windows)]
pub mod windows;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::{
    domain::{composer::ComposerRelease, errors::InfrastructureError, ports::composer::ComposerInstall},
    infrastructure::fs::atomic_write,
};

pub const PHAR_NAME: &str = "composer.phar";
const METADATA_NAME: &str = "open-herd.json";

/// `<base_dir>/composer`.
pub fn composer_dir(base_dir: &Path) -> PathBuf { base_dir.join("composer") }
/// `<base_dir>/bin`: the folder new terminals find `composer` in.
pub fn bin_dir(base_dir: &Path) -> PathBuf { base_dir.join("bin") }

#[derive(Serialize, Deserialize)]
struct Metadata {
    version: String,
    min_php: u32,
}

/// Records which release the phar in `dir` is (getcomposer.org's list is
/// the only place that says which PHP it needs).
pub fn write_metadata(dir: &Path, release: &ComposerRelease) -> std::io::Result<()> {
    let meta = Metadata { version: release.version.clone(), min_php: release.min_php };
    let json = serde_json::to_vec_pretty(&meta).map_err(std::io::Error::other)?;
    atomic_write(&dir.join(METADATA_NAME), &json)
}

pub fn read_metadata(dir: &Path) -> Option<ComposerInstall> {
    let meta: Metadata = serde_json::from_slice(&std::fs::read(dir.join(METADATA_NAME)).ok()?).ok()?;
    Some(ComposerInstall { version: Some(meta.version), min_php: Some(meta.min_php) })
}

/// The installed phar, if any: its recorded version, or "unknown" when the
/// phar was put there by hand.
pub fn installed(composer_dir: &Path) -> Option<ComposerInstall> {
    composer_dir.join(PHAR_NAME).is_file()
        .then(|| read_metadata(composer_dir).unwrap_or(ComposerInstall { version: None, min_php: None }))
}

const SHIM_NOTE: &str = "Created by Open Herd: runs Composer with the default PHP version. Rewritten when it changes in Settings.";

/// `composer.bat`. `%` is doubled: cmd would expand it even inside quotes.
#[cfg(any(windows, test))]
pub fn windows_shim(php: &Path, phar: &Path) -> String {
    let q = |p: &Path| p.to_string_lossy().replace('%', "%%");
    format!("@echo off\r\nrem {SHIM_NOTE}\r\n\"{}\" \"{}\" %*\r\n", q(php), q(phar))
}

/// The PHP a `composer.bat` written by [`windows_shim`] runs.
#[cfg(any(windows, test))]
pub fn windows_shim_php(text: &str) -> Option<PathBuf> {
    let line = text.lines().map(str::trim).find(|l| l.starts_with('"'))?;
    let php = line.strip_prefix('"')?.split('"').next()?;
    Some(PathBuf::from(php.replace("%%", "%")))
}

/// Single-quotes a path for `sh` (`'` becomes `'\''`).
#[cfg(any(not(windows), test))]
fn sh_quote(p: &Path) -> String {
    format!("'{}'", p.to_string_lossy().replace('\'', r"'\''"))
}

/// The `composer` shell script.
#[cfg(any(not(windows), test))]
pub fn unix_shim(php: &Path, phar: &Path) -> String {
    format!("#!/bin/sh\n# {SHIM_NOTE}\nexec {} {} \"$@\"\n", sh_quote(php), sh_quote(phar))
}

/// The PHP a script written by [`unix_shim`] runs.
#[cfg(any(not(windows), test))]
pub fn unix_shim_php(text: &str) -> Option<PathBuf> {
    let rest = text.lines().find_map(|l| l.trim().strip_prefix("exec "))?;
    let mut out = String::new();
    let mut chars = rest.strip_prefix('\'')?.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\'' { out.push(c); continue; }
        // `'\''` is an escaped quote; any other `'` ends the word.
        if chars.peek() != Some(&'\\') { return Some(PathBuf::from(out)); }
        let escaped: String = chars.by_ref().take(3).collect();
        if escaped != "\\''" { return None; }
        out.push('\'');
    }
    None
}

/// Module names from `php -m` output, lowercase, without the `[…]` headers.
pub fn parse_php_modules(output: &str) -> Vec<String> {
    output.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('['))
        .map(str::to_ascii_lowercase)
        .collect()
}

/// Runs `php -m`. Blocking: call through `blocking::run`.
pub fn php_modules_blocking(php: &Path) -> Result<Vec<String>, InfrastructureError> {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(php);
    cmd.arg("-m");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output()
        .map_err(|e| InfrastructureError::ProcessFailed(format!("{} -m: {e}", php.display())))?;
    if !out.status.success() {
        return Err(InfrastructureError::ProcessFailed(format!(
            "{} -m exited with {:?}: {}", php.display(), out.status.code(), String::from_utf8_lossy(&out.stderr).trim())));
    }
    Ok(parse_php_modules(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_shim_round_trips_the_php_path() {
        let php = Path::new(r"C:\Users\me\.phpenv\php\8.2\php.exe");
        let text = windows_shim(php, Path::new(r"C:\Users\me\.phpenv\composer\composer.phar"));
        assert!(text.starts_with("@echo off\r\n"));
        assert!(text.contains(r#""C:\Users\me\.phpenv\php\8.2\php.exe" "C:\Users\me\.phpenv\composer\composer.phar" %*"#), "{text}");
        assert_eq!(windows_shim_php(&text).as_deref(), Some(php));
    }

    #[test]
    fn windows_shim_escapes_percent() {
        let php = Path::new(r"C:\100%\php.exe");
        let text = windows_shim(php, Path::new(r"C:\x\composer.phar"));
        assert!(text.contains(r#""C:\100%%\php.exe""#), "{text}");
        assert_eq!(windows_shim_php(&text).as_deref(), Some(php));
    }

    #[test]
    fn unix_shim_quotes_and_round_trips() {
        let php = Path::new("/home/o'neil/.phpenv/php/8.4/php");
        let text = unix_shim(php, Path::new("/home/o'neil/.phpenv/composer/composer.phar"));
        assert!(text.starts_with("#!/bin/sh\n"));
        assert!(text.contains(r#"exec '/home/o'\''neil/.phpenv/php/8.4/php' '/home/o'\''neil/.phpenv/composer/composer.phar' "$@""#), "{text}");
        assert_eq!(unix_shim_php(&text).as_deref(), Some(php));
        assert_eq!(unix_shim_php("#!/bin/sh\nexec php x\n"), None, "not ours");
    }

    #[test]
    fn php_modules_skip_headers() {
        let out = "[PHP Modules]\nCore\ncurl\nmbstring\nZip\n\n[Zend Modules]\nZend OPcache\n";
        assert_eq!(parse_php_modules(out), ["core", "curl", "mbstring", "zip", "zend opcache"]);
    }

    #[test]
    fn metadata_round_trip_and_hand_copied_phar() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        assert_eq!(installed(dir), None);
        std::fs::write(dir.join(PHAR_NAME), b"phar").unwrap();
        assert_eq!(installed(dir), Some(ComposerInstall { version: None, min_php: None }));
        write_metadata(dir, &ComposerRelease::new("2.2.32", 50300, true).unwrap()).unwrap();
        assert_eq!(installed(dir), Some(ComposerInstall { version: Some("2.2.32".into()), min_php: Some(50300) }));
    }
}
