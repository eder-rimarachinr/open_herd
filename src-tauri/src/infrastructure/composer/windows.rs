//! The `composer` command on Windows: `<base_dir>\bin\composer.bat`, with
//! `<base_dir>\bin` appended once to the user PATH (HKCU, never the machine
//! PATH), like the Node manager's `current` link.

use async_trait::async_trait;
use std::{path::{Path, PathBuf}, sync::Arc};

use super::{bin_dir, composer_dir, installed, php_modules_blocking, windows_shim, windows_shim_php, PHAR_NAME};
use crate::{
    domain::{errors::InfrastructureError, ports::composer::{ComposerEnvironment, ComposerManagerPort}},
    infrastructure::{blocking, fs::atomic_write, node::user_path::{same_dir, UserPathEditor}},
};

/// File names cmd / PowerShell run for `composer`.
const COMMAND_NAMES: [&str; 3] = ["composer.bat", "composer.cmd", "composer.exe"];

pub struct WindowsComposerManager {
    base_dir: PathBuf,
    path: Arc<dyn UserPathEditor>,
}

impl WindowsComposerManager {
    /// The real machine: the HKCU PATH.
    pub fn detect(base_dir: PathBuf) -> Self {
        Self::with_path(base_dir, Arc::new(crate::infrastructure::node::user_path::RegistryUserPath))
    }

    pub fn with_path(base_dir: PathBuf, path: Arc<dyn UserPathEditor>) -> Self { Self { base_dir, path } }
}

struct Layout { composer_dir: PathBuf, bin_dir: PathBuf, command: PathBuf }

fn layout(base_dir: &Path) -> Layout {
    let bin = bin_dir(base_dir);
    Layout { composer_dir: composer_dir(base_dir), command: bin.join("composer.bat"), bin_dir: bin }
}

/// PATH folders before ours that hold another `composer` (Composer-Setup puts
/// one on the machine PATH, which Windows searches first).
fn shadowing_dirs(path_dirs: &[PathBuf], ours: &Path) -> Vec<PathBuf> {
    path_dirs.iter()
        .take_while(|d| !same_dir(d, ours))
        .filter(|d| COMMAND_NAMES.iter().any(|n| d.join(n).is_file()))
        .cloned()
        .collect()
}

fn warnings(l: &Layout, path_dirs: &[PathBuf]) -> Vec<String> {
    if !l.command.is_file() { return Vec::new(); }
    if !path_dirs.iter().any(|d| same_dir(d, &l.bin_dir)) {
        return vec![format!("{} is not on your user PATH any more: press Repair to add it back.", l.bin_dir.display())];
    }
    shadowing_dirs(path_dirs, &l.bin_dir).iter()
        .map(|d| format!(
            "Another Composer comes first on the PATH: {}. Terminals run that one; uninstall it (Composer-Setup) or remove that folder from the PATH to use Open Herd's.",
            d.display()))
        .collect()
}

fn environment_blocking(l: &Layout, path_dirs: &[PathBuf]) -> ComposerEnvironment {
    ComposerEnvironment {
        phar_path: l.composer_dir.join(PHAR_NAME),
        command_path: l.command.clone(),
        bin_dir: l.bin_dir.clone(),
        installed: installed(&l.composer_dir),
        command_php: std::fs::read_to_string(&l.command).ok().as_deref().and_then(windows_shim_php),
        warnings: warnings(l, path_dirs),
    }
}

fn write_command_blocking(l: &Layout, php: &Path, path: &dyn UserPathEditor) -> Result<(), InfrastructureError> {
    std::fs::create_dir_all(&l.bin_dir)?;
    atomic_write(&l.command, windows_shim(php, &l.composer_dir.join(PHAR_NAME)).as_bytes())?;
    path.ensure_entry(&l.bin_dir)?;
    Ok(())
}

#[async_trait]
impl ComposerManagerPort for WindowsComposerManager {
    async fn environment(&self) -> Result<ComposerEnvironment, InfrastructureError> {
        let (l, path) = (layout(&self.base_dir), self.path.clone());
        blocking::run(move || Ok(environment_blocking(&l, &path.dirs_on_path()))).await
    }

    async fn write_command(&self, php_binary: &Path) -> Result<(), InfrastructureError> {
        let (l, path, php) = (layout(&self.base_dir), self.path.clone(), php_binary.to_path_buf());
        blocking::run(move || write_command_blocking(&l, &php, path.as_ref())).await
    }

    async fn php_modules(&self, php_binary: &Path) -> Result<Vec<String>, InfrastructureError> {
        let php = php_binary.to_path_buf();
        blocking::run(move || php_modules_blocking(&php)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use tempfile::TempDir;

    /// The user PATH as a list; `ensure_entry` appends to it.
    #[derive(Default)]
    struct FakePath { dirs: Mutex<Vec<PathBuf>> }
    impl UserPathEditor for FakePath {
        fn ensure_entry(&self, dir: &Path) -> Result<bool, InfrastructureError> {
            let mut dirs = self.dirs.lock();
            if dirs.iter().any(|d| same_dir(d, dir)) { return Ok(false); }
            dirs.push(dir.to_path_buf());
            Ok(true)
        }
        fn dirs_on_path(&self) -> Vec<PathBuf> { self.dirs.lock().clone() }
    }

    fn installed_phar(base: &Path) {
        let dir = composer_dir(base);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(PHAR_NAME), b"phar").unwrap();
    }

    #[tokio::test]
    async fn writes_the_bat_and_adds_bin_to_the_user_path_once() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join("phpenv");
        let path = Arc::new(FakePath::default());
        let m = WindowsComposerManager::with_path(base.clone(), path.clone());
        installed_phar(&base);
        let php = PathBuf::from(r"C:\php\8.2\php.exe");

        m.write_command(&php).await.unwrap();
        m.write_command(&PathBuf::from(r"C:\php\8.4\php.exe")).await.unwrap();

        let env = m.environment().await.unwrap();
        assert_eq!(env.command_path, base.join("bin").join("composer.bat"));
        assert_eq!(env.command_php.as_deref(), Some(Path::new(r"C:\php\8.4\php.exe")), "rewritten, not appended");
        assert_eq!(*path.dirs.lock(), vec![base.join("bin")]);
        assert!(env.installed.is_some());
        assert!(env.warnings.is_empty(), "{:?}", env.warnings);
    }

    #[tokio::test]
    async fn warns_about_a_composer_earlier_on_the_path() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join("phpenv");
        let setup = tmp.path().join("ComposerSetup").join("bin");
        std::fs::create_dir_all(&setup).unwrap();
        std::fs::write(setup.join("composer.bat"), "@php composer.phar %*").unwrap();
        let path = Arc::new(FakePath { dirs: Mutex::new(vec![setup.clone()]) });
        let m = WindowsComposerManager::with_path(base.clone(), path);
        installed_phar(&base);
        m.write_command(Path::new(r"C:\php\8.2\php.exe")).await.unwrap();

        let warnings = m.environment().await.unwrap().warnings;
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains(&setup.display().to_string()), "{}", warnings[0]);
    }

    #[tokio::test]
    async fn warns_when_bin_was_removed_from_the_path() {
        let tmp = TempDir::new().unwrap();
        let base = tmp.path().join("phpenv");
        let path = Arc::new(FakePath::default());
        let m = WindowsComposerManager::with_path(base.clone(), path.clone());
        m.write_command(Path::new(r"C:\php\8.2\php.exe")).await.unwrap();
        path.dirs.lock().clear();
        let warnings = m.environment().await.unwrap().warnings;
        assert!(warnings.iter().any(|w| w.contains("not on your user PATH")), "{warnings:?}");
    }

    #[tokio::test]
    async fn nothing_installed_means_no_warnings() {
        let tmp = TempDir::new().unwrap();
        let m = WindowsComposerManager::with_path(tmp.path().to_path_buf(), Arc::new(FakePath::default()));
        let env = m.environment().await.unwrap();
        assert!(env.installed.is_none() && env.command_php.is_none() && env.warnings.is_empty());
    }
}
