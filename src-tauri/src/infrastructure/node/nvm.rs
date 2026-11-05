//! Finds an nvm-windows install: `NVM_HOME` (where versions live) and
//! `NVM_SYMLINK` (the link on the PATH that `nvm use` repoints).

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvmLocation {
    pub home: PathBuf,
    pub symlink: PathBuf,
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

// TODO(linux): nvm (nvm-sh) keeps versions in ~/.nvm/versions/node; not handled yet.
#[cfg(not(windows))]
pub fn detect() -> Option<NvmLocation> { None }

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
    fn expand_env_replaces_known_and_keeps_unknown() {
        let home = std::env::var("PATH").unwrap();
        assert_eq!(expand_env("%PATH%"), home);
        assert_eq!(expand_env("%NO_SUCH_VAR_123%\\b;50%"), "%NO_SUCH_VAR_123%\\b;50%");
    }
}
