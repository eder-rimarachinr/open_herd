use std::path::{Path, PathBuf};
use crate::domain::site::value_objects::DomainName;
use crate::infrastructure::fs;

#[cfg(target_os = "windows")]
pub fn hosts_path() -> PathBuf { PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts") }
#[cfg(not(target_os = "windows"))]
pub fn hosts_path() -> PathBuf { PathBuf::from("/etc/hosts") }

const MARKER: &str = "# open-herd";

/// Serialises every read-modify-write of the hosts file. Without it, a scan and
/// a manual create running at once can each read the old file and the second
/// write drops the first one's entry.
static HOSTS_LOCK: parking_lot::Mutex<()> = parking_lot::const_mutex(());

fn is_our_entry(line: &str, domain: &str) -> bool {
    if !line.contains(MARKER) { return false; }
    let mut fields = line.split_whitespace();
    fields.next();
    fields.next().is_some_and(|d| d == domain)
}

fn detect_line_ending(content: &str) -> &'static str {
    if content.contains("\r\n") { "\r\n" } else { "\n" }
}

/// One change to the hosts file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostsEdit {
    Add(Vec<String>),
    Remove(String),
}

impl HostsEdit {
    pub fn describe(&self) -> String {
        match self {
            Self::Add(d) => format!("add {}", d.join(", ")),
            Self::Remove(d) => format!("remove {d}"),
        }
    }
}

#[derive(Debug)]
pub enum HostsError {
    /// Only an administrator may write the file (Windows: the app is not elevated).
    NeedsElevation,
    Other(String),
}

impl std::fmt::Display for HostsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NeedsElevation => f.write_str(PERMISSION_DENIED),
            Self::Other(e) => f.write_str(e),
        }
    }
}

const PERMISSION_DENIED: &str = "Permission denied writing hosts file. Run the app as Administrator.";

/// Applies `edit` to the system hosts file with this process's rights.
pub fn edit(edit: &HostsEdit) -> Result<(), HostsError> {
    edit_at(&hosts_path(), edit, write_hosts)
}

/// The new file content, or `None` when the edit changes nothing.
fn apply(content: &str, edit: &HostsEdit) -> Result<Option<String>, String> {
    let eol = detect_line_ending(content);
    match edit {
        HostsEdit::Add(domains) => {
            let mut missing: Vec<&str> = vec![];
            for d in domains {
                DomainName::new(d).map_err(|e| format!("Invalid domain: {e}"))?;
                if !content.lines().any(|l| is_our_entry(l, d)) && !missing.contains(&d.as_str()) {
                    missing.push(d);
                }
            }
            if missing.is_empty() { return Ok(None); }
            let mut out = content.trim_end().to_string();
            for d in missing { out.push_str(&format!("{eol}127.0.0.1\t{d}\t{MARKER}")); }
            out.push_str(eol);
            Ok(Some(out))
        }
        HostsEdit::Remove(domain) => {
            if !content.lines().any(|l| is_our_entry(l, domain)) { return Ok(None); }
            let mut out = content.lines()
                .filter(|l| !is_our_entry(l, domain))
                .collect::<Vec<_>>()
                .join(eol);
            out.push_str(eol);
            Ok(Some(out))
        }
    }
}

/// Read, apply and write under [`HOSTS_LOCK`]. `write` is the direct write
/// ([`write_hosts`]) or, on Windows, the elevated helper.
pub(super) fn edit_at(
    path: &Path,
    edit: &HostsEdit,
    write: impl FnOnce(&Path, &str) -> Result<(), HostsError>,
) -> Result<(), HostsError> {
    let _guard  = HOSTS_LOCK.lock();
    let content = read_hosts(path).map_err(HostsError::Other)?;
    match apply(&content, edit).map_err(HostsError::Other)? {
        Some(new_content) => write(path, &new_content),
        None => Ok(()),
    }
}

fn read_hosts(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("Cannot read hosts file: {}", e))
}

/// Direct write with this process's rights; `NeedsElevation` when the backup
/// or the file itself may not be written.
pub(super) fn write_hosts(path: &Path, content: &str) -> Result<(), HostsError> {
    replace_hosts(path, content).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => HostsError::NeedsElevation,
        _ => HostsError::Other(format!("Cannot write hosts file: {e}")),
    })
}

/// The hosts file is a system file: keep a copy of the last known-good version
/// next to it, then replace it atomically so a crash mid-write cannot leave it
/// truncated. Also what the elevated helper runs.
pub(super) fn replace_hosts(path: &Path, content: &str) -> std::io::Result<()> {
    let backup = fs::sibling(path, ".open-herd.bak");
    std::fs::copy(path, &backup)?;
    match fs::atomic_write(path, content.as_bytes()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Err(e),
        // Rename-over can be refused for a system file (e.g. an antivirus holding
        // a handle). Fall back to an in-place write; the backup above covers it.
        Err(_) => std::fs::write(path, content),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn add_entry_at(path: &Path, domain: &str) -> Result<(), HostsError> {
        edit_at(path, &HostsEdit::Add(vec![domain.to_string()]), write_hosts)
    }

    fn remove_entry_at(path: &Path, domain: &str) -> Result<(), HostsError> {
        edit_at(path, &HostsEdit::Remove(domain.to_string()), write_hosts)
    }

    fn hosts_with(content: &str) -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("hosts");
        std::fs::write(&path, content).unwrap();
        (tmp, path)
    }

    #[test]
    fn add_then_remove_restores_foreign_lines() {
        let original = "127.0.0.1 localhost\n10.0.0.5 nas.lan\n";
        let (_tmp, path) = hosts_with(original);

        add_entry_at(&path, "myapp.test").unwrap();
        let added = std::fs::read_to_string(&path).unwrap();
        assert!(added.contains("127.0.0.1\tmyapp.test\t# open-herd"));
        assert!(added.contains("10.0.0.5 nas.lan"));

        remove_entry_at(&path, "myapp.test").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    }

    #[test]
    fn add_is_idempotent() {
        let (_tmp, path) = hosts_with("127.0.0.1 localhost\n");
        add_entry_at(&path, "a.test").unwrap();
        add_entry_at(&path, "a.test").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content.matches("a.test").count(), 1);
    }

    #[test]
    fn crlf_is_preserved() {
        let (_tmp, path) = hosts_with("127.0.0.1 localhost\r\n");
        add_entry_at(&path, "a.test").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content.matches('\n').count(), content.matches("\r\n").count());
    }

    #[test]
    fn previous_version_is_backed_up() {
        let (_tmp, path) = hosts_with("127.0.0.1 localhost\n");
        add_entry_at(&path, "a.test").unwrap();
        let backup = std::fs::read_to_string(fs::sibling(&path, ".open-herd.bak")).unwrap();
        assert_eq!(backup, "127.0.0.1 localhost\n");
    }

    #[test]
    fn removing_absent_entry_does_not_rewrite_file() {
        let (_tmp, path) = hosts_with("127.0.0.1 localhost");
        remove_entry_at(&path, "absent.test").unwrap();
        assert!(!fs::sibling(&path, ".open-herd.bak").exists());
    }

    #[test]
    fn several_entries_are_added_in_one_write() {
        let (_tmp, path) = hosts_with("127.0.0.1 localhost\r\n127.0.0.1\ta.test\t# open-herd\r\n");
        let edit = HostsEdit::Add(vec!["a.test".into(), "b.test".into(), "c.test".into(), "b.test".into()]);
        let mut writes = 0;
        edit_at(&path, &edit, |p, c| { writes += 1; write_hosts(p, c) }).unwrap();
        assert_eq!(writes, 1);
        let content = std::fs::read_to_string(&path).unwrap();
        for d in ["a.test", "b.test", "c.test"] {
            assert_eq!(content.matches(d).count(), 1, "{d} once");
        }
        assert_eq!(content.matches('\n').count(), content.matches("\r\n").count());
    }

    #[test]
    fn unchanged_file_is_not_written() {
        let (_tmp, path) = hosts_with("127.0.0.1\ta.test\t# open-herd\n");
        edit_at(&path, &HostsEdit::Add(vec!["a.test".into()]), |_, _| panic!("no write expected")).unwrap();
    }

    #[test]
    fn invalid_domain_is_refused_before_writing() {
        let (_tmp, path) = hosts_with("127.0.0.1 localhost\n");
        let err = edit_at(&path, &HostsEdit::Add(vec!["bad domain".into()]), |_, _| panic!("no write")).unwrap_err();
        assert!(matches!(err, HostsError::Other(m) if m.contains("Invalid domain")));
    }

    /// A read-only hosts file stands in for one only an administrator may write.
    #[cfg(windows)]
    #[test]
    #[allow(clippy::permissions_set_readonly_false)] // Windows only: clears the read-only flag
    fn permission_denied_asks_for_elevation() {
        let (_tmp, path) = hosts_with("127.0.0.1 localhost\n");
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&path, perms.clone()).unwrap();
        let err = add_entry_at(&path, "a.test").unwrap_err();
        perms.set_readonly(false);
        std::fs::set_permissions(&path, perms).unwrap();
        assert!(matches!(err, HostsError::NeedsElevation), "{err}");
    }

    #[test]
    fn concurrent_adds_do_not_lose_entries() {
        let (_tmp, path) = hosts_with("127.0.0.1 localhost\n");
        std::thread::scope(|s| {
            for i in 0..16 {
                let path = &path;
                s.spawn(move || add_entry_at(path, &format!("site{i}.test")).unwrap());
            }
        });
        let content = std::fs::read_to_string(&path).unwrap();
        for i in 0..16 {
            assert!(content.contains(&format!("site{i}.test")), "site{i}.test was lost");
        }
    }
}
