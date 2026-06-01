use std::path::PathBuf;
use crate::domain::site::value_objects::DomainName;

#[cfg(target_os = "windows")]
pub fn hosts_path() -> PathBuf { PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts") }
#[cfg(not(target_os = "windows"))]
pub fn hosts_path() -> PathBuf { PathBuf::from("/etc/hosts") }

const MARKER: &str = "# open-herd";

fn is_our_entry(line: &str, domain: &str) -> bool {
    if !line.contains(MARKER) { return false; }
    let mut fields = line.split_whitespace();
    fields.next();
    fields.next().map(|d| d == domain).unwrap_or(false)
}

fn detect_line_ending(content: &str) -> &'static str {
    if content.contains("\r\n") { "\r\n" } else { "\n" }
}

pub fn add_entry(domain: &str) -> Result<(), String> {
    // Validar con el value object del dominio
    DomainName::new(domain).map_err(|e| format!("Invalid domain: {}", e))?;
    let path    = hosts_path();
    let content = std::fs::read_to_string(&path).map_err(|e| format!("Cannot read hosts file: {}", e))?;
    if content.lines().any(|l| is_our_entry(l, domain)) { return Ok(()); }
    let eol     = detect_line_ending(&content);
    let entry   = format!("127.0.0.1\t{}\t{}", domain, MARKER);
    let new_content = format!("{}{}{}", content.trim_end(), eol, entry);
    write_hosts(&path, &new_content)
}

pub fn remove_entry(domain: &str) -> Result<(), String> {
    let path    = hosts_path();
    let content = std::fs::read_to_string(&path).map_err(|e| format!("Cannot read hosts file: {}", e))?;
    let eol     = detect_line_ending(&content);
    let filtered: Vec<&str> = content.lines().filter(|l| !is_our_entry(l, domain)).collect();
    write_hosts(&path, &filtered.join(eol))
}

fn write_hosts(path: &PathBuf, content: &str) -> Result<(), String> {
    std::fs::write(path, content).map_err(|e| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            "Permission denied writing hosts file. Run the app as Administrator.".into()
        } else {
            format!("Cannot write hosts file: {}", e)
        }
    })
}
