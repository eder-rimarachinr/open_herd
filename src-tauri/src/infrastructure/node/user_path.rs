//! The user's PATH (HKCU) and the machine PATH, read from the registry.

use std::path::{Path, PathBuf};

use crate::domain::errors::InfrastructureError;

/// Edits / inspects PATH. A trait so tests never touch the real registry.
pub trait UserPathEditor: Send + Sync {
    /// Appends `dir` to the user PATH unless it is already there; `true` if it
    /// was added. Never touches the machine PATH.
    fn ensure_entry(&self, dir: &Path) -> Result<bool, InfrastructureError>;
    /// Every PATH directory (expanded), in the order a new terminal searches
    /// them: machine PATH entries, then user PATH entries.
    fn dirs_on_path(&self) -> Vec<PathBuf>;
    /// [`Self::dirs_on_path`] entries that contain a `node.exe`.
    fn node_dirs_on_path(&self) -> Vec<PathBuf> {
        self.dirs_on_path().into_iter().filter(|d| d.join("node.exe").exists()).collect()
    }
}

/// Comparison of PATH entries that ignores case, `/` vs `\`, trailing
/// separators and doubled separators inside (nvm-windows 2.x writes
/// `...\nvm\\.nodejs`). A leading `\\` (UNC share) is kept.
pub fn same_dir(a: &Path, b: &Path) -> bool {
    fn norm(p: &Path) -> String {
        let s = p.to_string_lossy().trim().replace('/', "\\").to_lowercase();
        let (unc, rest) = match s.strip_prefix("\\\\") { Some(r) => ("\\\\", r), None => ("", s.as_str()) };
        let mut out = String::from(unc);
        for c in rest.chars() {
            if c == '\\' && out.ends_with('\\') && out.len() > unc.len() { continue; }
            out.push(c);
        }
        out.trim_end_matches('\\').to_string()
    }
    norm(a) == norm(b)
}

/// `path_value` with `dir` appended (`;`-separated), or `None` if already present.
pub fn with_entry(path_value: &str, dir: &Path) -> Option<String> {
    if path_value.split(';').any(|e| same_dir(Path::new(&super::nvm::expand_env(e)), dir)) { return None; }
    let trimmed = path_value.trim_end_matches(';');
    let dir = dir.to_string_lossy();
    Some(if trimmed.is_empty() { dir.into_owned() } else { format!("{trimmed};{dir}") })
}

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use winreg::{enums::*, RegKey, RegValue};

    const MACHINE_ENV: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

    /// A variable from HKCU\Environment, else from the machine environment.
    pub fn registry_env_var(name: &str) -> Option<String> {
        let read = |root: RegKey, path: &str| root.open_subkey(path).ok()?.get_value::<String, _>(name).ok();
        read(RegKey::predef(HKEY_CURRENT_USER), "Environment")
            .or_else(|| read(RegKey::predef(HKEY_LOCAL_MACHINE), MACHINE_ENV))
    }

    /// The real user PATH in `HKCU\Environment`.
    pub struct RegistryUserPath;

    fn utf16_to_string(bytes: &[u8]) -> String {
        let wide: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&wide).trim_end_matches('\0').to_string()
    }

    fn string_to_utf16(s: &str) -> Vec<u8> {
        s.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes).collect()
    }

    /// Tells Explorer (and so terminals opened afterwards) that the environment changed.
    fn broadcast_environment_change() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};
        let param: Vec<u16> = "Environment".encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `param` is a NUL-terminated UTF-16 string that outlives the call;
        // the result pointer may be null per the API contract.
        unsafe {
            SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, param.as_ptr() as isize, SMTO_ABORTIFHUNG, 2000, std::ptr::null_mut());
        }
    }

    impl UserPathEditor for RegistryUserPath {
        fn ensure_entry(&self, dir: &Path) -> Result<bool, InfrastructureError> {
            let (env, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey("Environment")?;
            let (text, vtype) = match env.get_raw_value("Path") {
                Ok(v) => (utf16_to_string(&v.bytes), v.vtype),
                Err(_) => (String::new(), REG_EXPAND_SZ),
            };
            let Some(updated) = with_entry(&text, dir) else { return Ok(false) };
            env.set_raw_value("Path", &RegValue { bytes: string_to_utf16(&updated).into(), vtype })?;
            broadcast_environment_change();
            Ok(true)
        }

        fn dirs_on_path(&self) -> Vec<PathBuf> {
            let read = |root: RegKey, key: &str| root.open_subkey(key)
                .and_then(|k| k.get_value::<String, _>("Path")).unwrap_or_default();
            let machine = read(RegKey::predef(HKEY_LOCAL_MACHINE), MACHINE_ENV);
            let user = read(RegKey::predef(HKEY_CURRENT_USER), "Environment");
            machine.split(';').chain(user.split(';'))
                .filter(|e| !e.trim().is_empty())
                .map(|e| PathBuf::from(super::super::nvm::expand_env(e.trim())))
                .collect()
        }
    }
}

#[cfg(windows)]
pub use windows_impl::{registry_env_var, RegistryUserPath};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_entry_appends_once() {
        let dir = Path::new(r"C:\Users\me\.phpenv\node\current");
        assert_eq!(with_entry(r"C:\a;C:\b;", dir).as_deref(), Some(r"C:\a;C:\b;C:\Users\me\.phpenv\node\current"));
        assert_eq!(with_entry("", dir).as_deref(), Some(r"C:\Users\me\.phpenv\node\current"));
        assert_eq!(with_entry(r"C:\a;c:\users\ME\.phpenv\node\current\", dir), None);
    }

    #[test]
    fn same_dir_ignores_doubled_separators() {
        let link = Path::new(r"C:\Users\me\AppData\Local\Author Software\nvm\.nodejs");
        assert!(same_dir(Path::new(r"C:\Users\me\AppData\Local\Author Software\nvm\\.nodejs"), link));
        assert!(same_dir(Path::new("C:/Users/me/AppData/Local/Author Software/nvm/.nodejs/"), link));
        assert!(!same_dir(Path::new(r"C:\Users\me\AppData\Local\Author Software\nvm"), link));
        assert!(same_dir(Path::new(r"\\server\share\bin"), Path::new(r"\\SERVER\share\bin\")));
        assert!(!same_dir(Path::new(r"\\server\share"), Path::new(r"\server\share")));
    }
}
