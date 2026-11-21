//! The `composer` command on Linux: `<base_dir>/bin/composer` (an `sh` script,
//! mode 755) linked from `~/.local/bin/composer`. A real file there is never
//! replaced, only reported.

use async_trait::async_trait;
use std::{path::{Path, PathBuf}, sync::Arc};

use super::{bin_dir, composer_dir, installed, php_modules_blocking, unix_shim, unix_shim_php, PHAR_NAME};
use crate::{
    domain::{errors::InfrastructureError, ports::composer::{ComposerEnvironment, ComposerManagerPort}},
    infrastructure::{blocking, fs::atomic_write},
};

type PathDirs = Arc<dyn Fn() -> Vec<PathBuf> + Send + Sync>;

pub struct LinuxComposerManager {
    base_dir: PathBuf,
    /// `~/.local/bin`.
    local_bin: PathBuf,
    /// The PATH new shells get, for the "another Composer comes first" warning.
    path_dirs: PathDirs,
}

impl LinuxComposerManager {
    /// The real machine: `~/.local/bin` and this process's PATH.
    pub fn detect(base_dir: PathBuf) -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| base_dir.clone());
        Self::with_layout(
            base_dir,
            home.join(".local").join("bin"),
            Arc::new(|| std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default()),
        )
    }

    pub fn with_layout(base_dir: PathBuf, local_bin: PathBuf, path_dirs: PathDirs) -> Self {
        Self { base_dir, local_bin, path_dirs }
    }
}

#[derive(Clone)]
struct Layout { composer_dir: PathBuf, bin_dir: PathBuf, command: PathBuf, link: PathBuf, local_bin: PathBuf }

impl LinuxComposerManager {
    fn layout(&self) -> Layout {
        let bin = bin_dir(&self.base_dir);
        Layout {
            composer_dir: composer_dir(&self.base_dir),
            command: bin.join("composer"),
            bin_dir: bin,
            link: self.local_bin.join("composer"),
            local_bin: self.local_bin.clone(),
        }
    }
}

fn is_symlink(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
}

fn same(a: &Path, b: &Path) -> bool {
    a == b || a.canonicalize().ok().zip(b.canonicalize().ok()).is_some_and(|(x, y)| x == y)
}

fn warnings(l: &Layout, path_dirs: &[PathBuf]) -> Vec<String> {
    if !l.command.is_file() { return Vec::new(); }
    if l.link.exists() && !is_symlink(&l.link) {
        return vec![format!("{} is a real file, not an Open Herd link: it is left alone, and that `composer` wins over Open Herd's.", l.link.display())];
    }
    if !path_dirs.iter().any(|d| same(d, &l.local_bin)) {
        return vec![format!("{} is not on your PATH: log out and back in (Ubuntu adds it at login when the folder exists) or add it in ~/.profile.", l.local_bin.display())];
    }
    match path_dirs.iter().find(|d| d.join("composer").is_file()) {
        Some(first) if !same(first, &l.local_bin) && !same(first, &l.bin_dir) =>
            vec![format!("Another Composer comes first on the PATH: {}. Shells run that one.", first.join("composer").display())],
        _ => Vec::new(),
    }
}

fn environment_blocking(l: &Layout, path_dirs: &[PathBuf]) -> ComposerEnvironment {
    ComposerEnvironment {
        phar_path: l.composer_dir.join(PHAR_NAME),
        command_path: l.command.clone(),
        bin_dir: l.bin_dir.clone(),
        installed: installed(&l.composer_dir),
        command_php: std::fs::read_to_string(&l.command).ok().as_deref().and_then(unix_shim_php),
        warnings: warnings(l, path_dirs),
    }
}

fn write_command_blocking(l: &Layout, php: &Path) -> Result<(), InfrastructureError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(&l.bin_dir)?;
    atomic_write(&l.command, unix_shim(php, &l.composer_dir.join(PHAR_NAME)).as_bytes())?;
    std::fs::set_permissions(&l.command, std::fs::Permissions::from_mode(0o755))?;
    link_command(&l.link, &l.command)
}

/// `~/.local/bin/composer` → our script, swapped atomically. A real file is
/// left alone (reported by `warnings`).
fn link_command(link: &Path, target: &Path) -> Result<(), InfrastructureError> {
    if link.exists() && !is_symlink(link) { return Ok(()); }
    if is_symlink(link) && std::fs::read_link(link).is_ok_and(|t| t == target) { return Ok(()); }
    if let Some(parent) = link.parent() { std::fs::create_dir_all(parent)?; }
    let mut staging = link.as_os_str().to_owned();
    staging.push(".open-herd-new");
    let staging = PathBuf::from(staging);
    if is_symlink(&staging) { std::fs::remove_file(&staging)?; }
    std::os::unix::fs::symlink(target, &staging)?;
    if let Err(e) = std::fs::rename(&staging, link) {
        let _ = std::fs::remove_file(&staging);
        return Err(e.into());
    }
    Ok(())
}

#[async_trait]
impl ComposerManagerPort for LinuxComposerManager {
    async fn environment(&self) -> Result<ComposerEnvironment, InfrastructureError> {
        let (l, dirs) = (self.layout(), (self.path_dirs)());
        blocking::run(move || Ok(environment_blocking(&l, &dirs))).await
    }

    async fn write_command(&self, php_binary: &Path) -> Result<(), InfrastructureError> {
        let (l, php) = (self.layout(), php_binary.to_path_buf());
        blocking::run(move || write_command_blocking(&l, &php)).await
    }

    async fn php_modules(&self, php_binary: &Path) -> Result<Vec<String>, InfrastructureError> {
        let php = php_binary.to_path_buf();
        blocking::run(move || php_modules_blocking(&php)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn manager(tmp: &TempDir, path: Vec<PathBuf>) -> (LinuxComposerManager, PathBuf, PathBuf) {
        let (base, local) = (tmp.path().join("phpenv"), tmp.path().join(".local/bin"));
        (LinuxComposerManager::with_layout(base.clone(), local.clone(), Arc::new(move || path.clone())), base, local)
    }

    #[tokio::test]
    async fn writes_an_executable_script_and_links_it() {
        let tmp = TempDir::new().unwrap();
        let local = tmp.path().join(".local/bin");
        let (m, base, _) = manager(&tmp, vec![local.clone(), PathBuf::from("/usr/bin")]);
        m.write_command(Path::new("/x/php/8.4/php")).await.unwrap();
        let script = base.join("bin/composer");
        assert_eq!(std::fs::metadata(&script).unwrap().permissions().mode() & 0o777, 0o755);
        assert_eq!(std::fs::read_link(local.join("composer")).unwrap(), script);
        m.write_command(Path::new("/x/php/8.3/php")).await.unwrap();
        let env = m.environment().await.unwrap();
        assert_eq!(env.command_php.as_deref(), Some(Path::new("/x/php/8.3/php")));
        assert!(env.warnings.is_empty(), "{:?}", env.warnings);
    }

    #[tokio::test]
    async fn never_replaces_a_real_file_in_local_bin() {
        let tmp = TempDir::new().unwrap();
        let local = tmp.path().join(".local/bin");
        std::fs::create_dir_all(&local).unwrap();
        std::fs::write(local.join("composer"), "#!/bin/sh\necho mine\n").unwrap();
        let (m, _, _) = manager(&tmp, vec![local.clone()]);
        m.write_command(Path::new("/x/php")).await.unwrap();
        assert_eq!(std::fs::read_to_string(local.join("composer")).unwrap(), "#!/bin/sh\necho mine\n");
        let warnings = m.environment().await.unwrap().warnings;
        assert!(warnings[0].contains("real file"), "{warnings:?}");
    }

    #[tokio::test]
    async fn warns_about_a_composer_earlier_on_the_path() {
        let tmp = TempDir::new().unwrap();
        let other = tmp.path().join("usr-bin");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("composer"), "").unwrap();
        let local = tmp.path().join(".local/bin");
        let (m, _, _) = manager(&tmp, vec![other.clone(), local]);
        m.write_command(Path::new("/x/php")).await.unwrap();
        let warnings = m.environment().await.unwrap().warnings;
        assert!(warnings[0].contains("Another Composer"), "{warnings:?}");
    }
}
