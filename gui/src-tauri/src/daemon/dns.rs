use std::path::PathBuf;

#[cfg(target_os = "windows")]
pub fn hosts_path() -> PathBuf {
    PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
}

#[cfg(not(target_os = "windows"))]
pub fn hosts_path() -> PathBuf {
    PathBuf::from("/etc/hosts")
}

const MARKER: &str = "# open-herd";

/// Returns true only when a line is our managed entry for exactly `domain`.
/// Uses field-level matching (split on whitespace) instead of substring search
/// so that "app.test" cannot match "myapp.test".
fn is_our_entry(line: &str, domain: &str) -> bool {
    if !line.contains(MARKER) {
        return false;
    }
    let mut fields = line.split_whitespace();
    fields.next(); // skip IP
    fields.next().map(|d| d == domain).unwrap_or(false)
}

/// Detect the line ending convention used in the file so we can preserve it.
fn detect_line_ending(content: &str) -> &'static str {
    if content.contains("\r\n") { "\r\n" } else { "\n" }
}

pub fn add_entry(domain: &str) -> Result<(), String> {
    // Validate domain before touching the system file — prevents nginx config injection
    // via newlines or special characters embedded in the domain name.
    super::validate::domain(domain).map_err(|e| format!("Invalid domain: {}", e))?;

    let path = hosts_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read hosts file: {}", e))?;

    // Exact-match duplicate check — avoids false positives from substring search
    if content.lines().any(|l| is_our_entry(l, domain)) {
        return Ok(());
    }

    let eol = detect_line_ending(&content);
    let entry = format!("127.0.0.1\t{}\t{}", domain, MARKER);
    let new_content = format!("{}{}{}", content.trim_end(), eol, entry);
    write_hosts(&path, &new_content)
}

pub fn remove_entry(domain: &str) -> Result<(), String> {
    let path = hosts_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read hosts file: {}", e))?;

    // Preserve the file's original line ending (CRLF on Windows, LF on Linux)
    let eol = detect_line_ending(&content);

    // Exact-match filter — only removes lines that are our entry for this specific domain
    let filtered: Vec<&str> = content
        .lines()
        .filter(|l| !is_our_entry(l, domain))
        .collect();

    let new_content = filtered.join(eol);
    write_hosts(&path, &new_content)
}

fn write_hosts(path: &PathBuf, content: &str) -> Result<(), String> {
    std::fs::write(path, content)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                "Permission denied writing hosts file. Run the app as Administrator.".into()
            } else {
                format!("Cannot write hosts file: {}", e)
            }
        })
}
