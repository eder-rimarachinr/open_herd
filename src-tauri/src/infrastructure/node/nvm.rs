//! Finds an nvm-windows install. Two generations:
//! - 1.x: `NVM_HOME` (where versions live) and `NVM_SYMLINK` (the link on the
//!   PATH that `nvm use` repoints), from the environment or `settings.txt`.
//! - 2.x (Author Software): no environment variables; its settings live in
//!   `HKCU\Software\Author Software\Preferences\nvm` (`InstallRoot`,
//!   `ActiveVersion`). The PATH holds `<nvm>\.nodejs`, which points at
//!   launchers that run the active version: only `nvm use` may change it.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvmLocation {
    pub home: PathBuf,
    pub symlink: PathBuf,
}

/// An nvm-windows 2.x install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nvm2Location {
    /// The folder of `nvm.exe` (and of `.nodejs`, the PATH entry).
    pub dir: PathBuf,
    /// `InstallRoot`: one `v<version>` folder per install.
    pub install_root: PathBuf,
    /// `ActiveVersion` (`24.21.0`): the default version new terminals run.
    pub active: Option<String>,
}

impl Nvm2Location {
    pub fn exe(&self) -> PathBuf { self.dir.join("nvm.exe") }
    /// The PATH entry that runs the active version.
    pub fn link(&self) -> PathBuf { self.dir.join(".nodejs") }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NvmInstall {
    Classic(NvmLocation),
    V2(Nvm2Location),
}

/// nvm 2.x's own folder: the parent of `InstallRoot` (its default layout,
/// `<nvm>\installs`) or the default install folder, whichever has `nvm.exe`.
pub fn nvm2_dir(install_root: &Path, local_app_data: Option<&Path>) -> Option<PathBuf> {
    let default = local_app_data.map(|d| d.join("Author Software").join("nvm"));
    install_root.parent().map(Path::to_path_buf).into_iter()
        .chain(default)
        .find(|d| d.join("nvm.exe").is_file())
}

/// nvm-windows 2.x first (its registry settings are current even if a 1.x
/// install left `NVM_HOME` behind), then 1.x.
#[cfg(windows)]
pub fn detect_any() -> Option<NvmInstall> {
    detect_v2().map(NvmInstall::V2).or_else(|| detect().map(NvmInstall::Classic))
}

#[cfg(windows)]
fn detect_v2() -> Option<Nvm2Location> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey(r"Software\Author Software\Preferences\nvm").ok()?;
    let root: String = key.get_value("InstallRoot").ok()?;
    let install_root = PathBuf::from(expand_env(root.trim()));
    if !install_root.is_dir() { return None; }
    let local_app_data = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let dir = nvm2_dir(&install_root, local_app_data.as_deref())?;
    let active = key.get_value::<String, _>("ActiveVersion").ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    Some(Nvm2Location { dir, install_root, active })
}

/// Reads `root:` and `path:` from nvm-windows' `settings.txt`.
pub fn parse_nvm_settings(text: &str) -> Option<NvmLocation> {
    let value = |key: &str| text.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        (k.trim().eq_ignore_ascii_case(key) && !v.trim().is_empty()).then(|| PathBuf::from(v.trim()))
    });
    Some(NvmLocation { home: value("root")?, symlink: value("path")? })
}

/// Expands `%VAR%` references with the process environment (registry values
/// are often stored unexpanded). Unknown variables are left as they are.
pub fn expand_env(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('%') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = &after[..end];
        match std::env::var(name) {
            Ok(v) if !name.is_empty() => out.push_str(&v),
            _ => { out.push('%'); out.push_str(name); out.push('%'); }
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Looks nvm-windows up on this machine: the registry (current values; the
/// app's own environment may predate an nvm install), then the process
/// environment, then `settings.txt` for whatever is still missing.
#[cfg(windows)]
pub fn detect() -> Option<NvmLocation> {
    let var = |name: &str| super::user_path::registry_env_var(name)
        .or_else(|| std::env::var(name).ok())
        .map(|v| PathBuf::from(expand_env(&v)))
        .filter(|p| !p.as_os_str().is_empty());
    let home = var("NVM_HOME")?;
    if !home.is_dir() { return None; }
    let from_settings = std::fs::read_to_string(home.join("settings.txt")).ok()
        .and_then(|t| parse_nvm_settings(&t));
    let symlink = var("NVM_SYMLINK").or_else(|| from_settings.map(|s| s.symlink))?;
    Some(NvmLocation { home, symlink })
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_nvm_settings_reads_root_and_path() {
        let text = "root: C:\\Users\\me\\AppData\\Local\\nvm\r\npath: C:\\nvm4w\\nodejs\r\nproxy: none\r\n";
        assert_eq!(parse_nvm_settings(text), Some(NvmLocation {
            home: PathBuf::from("C:\\Users\\me\\AppData\\Local\\nvm"),
            symlink: PathBuf::from("C:\\nvm4w\\nodejs"),
        }));
        assert_eq!(parse_nvm_settings("root: C:\\nvm\n"), None, "path: is required");
    }

    #[test]
    fn nvm2_dir_prefers_the_install_roots_parent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let nvm = tmp.path().join("Author Software").join("nvm");
        std::fs::create_dir_all(nvm.join("installs")).unwrap();
        assert_eq!(nvm2_dir(&nvm.join("installs"), Some(tmp.path())), None, "no nvm.exe anywhere");
        std::fs::write(nvm.join("nvm.exe"), b"exe").unwrap();
        assert_eq!(nvm2_dir(&nvm.join("installs"), None), Some(nvm.clone()));
        // A custom InstallRoot elsewhere: fall back to the default folder.
        let custom = tmp.path().join("D").join("node-versions");
        std::fs::create_dir_all(&custom).unwrap();
        assert_eq!(nvm2_dir(&custom, Some(tmp.path())), Some(nvm));
    }

    #[test]
    fn expand_env_replaces_known_and_keeps_unknown() {
        let home = std::env::var("PATH").unwrap();
        assert_eq!(expand_env("%PATH%"), home);
        assert_eq!(expand_env("%NO_SUCH_VAR_123%\\b;50%"), "%NO_SUCH_VAR_123%\\b;50%");
    }
}
