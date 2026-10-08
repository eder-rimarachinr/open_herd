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

pub fn add_entry(domain: &str) -> Result<(), String> {
    add_entry_at(&hosts_path(), domain)
}

pub fn remove_entry(domain: &str) -> Result<(), String> {
    remove_entry_at(&hosts_path(), domain)
}

fn add_entry_at(path: &Path, domain: &str) -> Result<(), String> {
    // Validar con el value object del dominio
    DomainName::new(domain).map_err(|e| format!("Invalid domain: {}", e))?;
    let _guard  = HOSTS_LOCK.lock();
    let content = read_hosts(path)?;
    if content.lines().any(|l| is_our_entry(l, domain)) { return Ok(()); }
    let eol     = detect_line_ending(&content);
    let new_content = format!("{}{eol}127.0.0.1\t{domain}\t{MARKER}{eol}", content.trim_end());
    write_hosts(path, &new_content)
}

fn remove_entry_at(path: &Path, domain: &str) -> Result<(), String> {
    let _guard  = HOSTS_LOCK.lock();
    let content = read_hosts(path)?;
    if !content.lines().any(|l| is_our_entry(l, domain)) { return Ok(()); }
    let eol     = detect_line_ending(&content);
    let mut new_content = content.lines()
        .filter(|l| !is_our_entry(l, domain))
        .collect::<Vec<_>>()
        .join(eol);
    new_content.push_str(eol);
    write_hosts(path, &new_content)
}

fn read_hosts(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("Cannot read hosts file: {}", e))
}

/// The hosts file is a system file: keep a copy of the last known-good version
/// next to it, then replace it atomically so a crash mid-write cannot leave it
/// truncated.
fn write_hosts(path: &Path, content: &str) -> Result<(), String> {
    let backup = fs::sibling(path, ".open-herd.bak");
    std::fs::copy(path, &backup).map_err(|e| describe_write_error(&e, "back up"))?;
    match fs::atomic_write(path, content.as_bytes()) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            Err(describe_write_error(&e, "write"))
        }
        // Rename-over can be refused for a system file (e.g. an antivirus holding
        // a handle). Fall back to an in-place write; the backup above covers it.
        Err(_) => std::fs::write(path, content).map_err(|e| describe_write_error(&e, "write")),
    }
}

fn describe_write_error(e: &std::io::Error, action: &str) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        "Permission denied writing hosts file. Run the app as Administrator.".into()
    } else {
        format!("Cannot {action} hosts file: {e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
