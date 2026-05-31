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

pub fn add_entry(domain: &str) -> Result<(), String> {
    let path = hosts_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read hosts file: {}", e))?;

    let entry = format!("127.0.0.1\t{}\t{}", domain, MARKER);

    // Already exists
    if content.lines().any(|l| {
        let l = l.trim();
        l.contains(domain) && l.contains(MARKER)
    }) {
        return Ok(());
    }

    let new_content = format!("{}\n{}", content.trim_end(), entry);
    write_hosts(&path, &new_content)
}

pub fn remove_entry(domain: &str) -> Result<(), String> {
    let path = hosts_path();
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Cannot read hosts file: {}", e))?;

    let filtered: Vec<&str> = content
        .lines()
        .filter(|l| !(l.contains(domain) && l.contains(MARKER)))
        .collect();

    let new_content = filtered.join("\n");
    write_hosts(&path, &new_content)
}

fn write_hosts(path: &PathBuf, content: &str) -> Result<(), String> {
    // On Windows the hosts file needs admin — try directly first
    std::fs::write(path, content)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                "Permission denied writing hosts file. Run the app as Administrator.".into()
            } else {
                format!("Cannot write hosts file: {}", e)
            }
        })
}
