//! Filesystem helpers shared by the adapters that own files on disk
//! (`sites.json`, `config.json`, `php.ini`, the hosts file).

use serde::de::DeserializeOwned;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Writes `data` to `path` so that a crash leaves either the old or the new
/// content, never a truncated file: write + fsync a sibling temp file, then
/// rename it over the target.
pub fn atomic_write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = sibling(path, ".tmp");
    let result = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(data)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Result of [`load_json_or`]. `warning` is set whenever the file existed but
/// could not be used, so the caller can surface it to the user.
pub struct Loaded<T> {
    pub value: T,
    pub warning: Option<String>,
}

/// Loads `path` as JSON, falling back to `default()` when the file is missing.
///
/// A file that exists but cannot be read or parsed is renamed to
/// `<name>.corrupt-<timestamp>` before falling back, so the next save cannot
/// overwrite the user's data.
pub fn load_json_or<T: DeserializeOwned>(path: &Path, default: impl FnOnce() -> T) -> Loaded<T> {
    let reason = match std::fs::read_to_string(path) {
        Ok(data) => match serde_json::from_str(&data) {
            Ok(value) => return Loaded { value, warning: None },
            Err(e) => format!("invalid JSON: {e}"),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Loaded { value: default(), warning: None };
        }
        Err(e) => format!("unreadable: {e}"),
    };
    let warning = match quarantine(path) {
        Ok(moved_to) => format!(
            "{} was {reason}; moved it to {} and started with defaults",
            path.display(), moved_to.display(),
        ),
        Err(e) => format!(
            "{} was {reason} and could not be moved aside ({e}); started with defaults \
             — the next save may overwrite it, back it up manually",
            path.display(),
        ),
    };
    eprintln!("[open-herd] {warning}");
    Loaded { value: default(), warning: Some(warning) }
}

/// Renames `path` to `<name>.corrupt-<timestamp>` and returns the new path.
fn quarantine(path: &Path) -> std::io::Result<PathBuf> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dest = sibling(path, &format!(".corrupt-{stamp}"));
    std::fs::rename(path, &dest)?;
    Ok(dest)
}

/// `dir/name` → `dir/name<suffix>`.
pub fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn corrupt_files(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir).unwrap()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.to_string_lossy().contains(".corrupt-"))
            .collect()
    }

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_temp_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("data.json");
        std::fs::write(&path, "old").unwrap();

        atomic_write(&path, b"new").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert!(!sibling(&path, ".tmp").exists());
    }

    #[test]
    fn atomic_write_into_missing_dir_fails_without_side_effects() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("missing").join("data.json");
        assert!(atomic_write(&path, b"x").is_err());
        assert!(!path.exists());
    }

    #[test]
    fn missing_file_yields_default_without_warning() {
        let tmp = TempDir::new().unwrap();
        let loaded: Loaded<Vec<u32>> = load_json_or(&tmp.path().join("nope.json"), Vec::new);
        assert!(loaded.value.is_empty());
        assert!(loaded.warning.is_none());
    }

    #[test]
    fn valid_file_is_loaded() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("ok.json");
        std::fs::write(&path, "[1,2,3]").unwrap();
        let loaded: Loaded<Vec<u32>> = load_json_or(&path, Vec::new);
        assert_eq!(loaded.value, vec![1, 2, 3]);
        assert!(loaded.warning.is_none());
    }

    #[test]
    fn corrupt_file_is_quarantined_with_its_content_intact() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("sites.json");
        std::fs::write(&path, "[{\"id\": truncated").unwrap();

        let loaded: Loaded<Vec<u32>> = load_json_or(&path, Vec::new);

        assert!(loaded.value.is_empty());
        assert!(loaded.warning.is_some());
        assert!(!path.exists(), "the corrupt file must be moved out of the way");
        let moved = corrupt_files(tmp.path());
        assert_eq!(moved.len(), 1);
        assert_eq!(std::fs::read_to_string(&moved[0]).unwrap(), "[{\"id\": truncated");
    }
}
