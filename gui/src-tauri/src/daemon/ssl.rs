use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::Mutex;

use super::models::AsyncTask;

// ── mkcert binary ─────────────────────────────────────────────────────────────

const MKCERT_VERSION: &str = "v1.4.4";

#[cfg(target_os = "windows")]
const MKCERT_URL: &str =
    "https://github.com/FiloSottile/mkcert/releases/download/v1.4.4/mkcert-v1.4.4-windows-amd64.exe";

#[cfg(not(target_os = "windows"))]
const MKCERT_URL: &str =
    "https://github.com/FiloSottile/mkcert/releases/download/v1.4.4/mkcert-v1.4.4-linux-amd64";

pub fn mkcert_path(base_dir: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    return base_dir.join("mkcert.exe");
    #[cfg(not(target_os = "windows"))]
    return base_dir.join("mkcert");
}

/// Download mkcert if not already present, make it executable.
pub fn ensure_mkcert(base_dir: &Path) -> Result<PathBuf, String> {
    let path = mkcert_path(base_dir);
    if path.exists() {
        return Ok(path);
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("open-herd/0.1")
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(MKCERT_URL)
        .send()
        .map_err(|e| format!("Failed to download mkcert {}: {}", MKCERT_VERSION, e))?;

    if !resp.status().is_success() {
        return Err(format!("mkcert download returned HTTP {}", resp.status()));
    }

    let bytes = resp.bytes().map_err(|e| e.to_string())?;
    std::fs::write(&path, &bytes)
        .map_err(|e| format!("Failed to save mkcert: {}", e))?;

    // Make executable on Linux / macOS
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("Failed to set mkcert permissions: {}", e))?;
    }

    Ok(path)
}

/// Install the mkcert local CA into the system/browser trust stores.
/// Only needs to run once per machine; safe to call multiple times.
pub fn install_ca(mkcert: &Path) -> Result<(), String> {
    let output = std::process::Command::new(mkcert)
        .arg("-install")
        .output()
        .map_err(|e| format!("Failed to run mkcert -install: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("mkcert -install failed: {}", stderr.trim()));
    }
    Ok(())
}

// ── Cert issuance ─────────────────────────────────────────────────────────────

pub struct CertPaths {
    pub cert: PathBuf,
    pub key:  PathBuf,
}

/// Issue a certificate for `domain`, writing cert + key into `certs_dir`.
/// Files are named `<domain>.pem` and `<domain>-key.pem`.
pub fn issue_cert(mkcert: &Path, domain: &str, certs_dir: &Path) -> Result<CertPaths, String> {
    std::fs::create_dir_all(certs_dir)
        .map_err(|e| format!("Failed to create certs dir: {}", e))?;

    let cert = certs_dir.join(format!("{}.pem", domain));
    let key  = certs_dir.join(format!("{}-key.pem", domain));

    let output = std::process::Command::new(mkcert)
        .args([
            "-cert-file", &cert.to_string_lossy().to_string(),
            "-key-file",  &key.to_string_lossy().to_string(),
            domain,
        ])
        .output()
        .map_err(|e| format!("Failed to run mkcert: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("mkcert cert issuance failed: {}", stderr.trim()));
    }

    Ok(CertPaths { cert, key })
}

/// Remove cert files for a domain. Non-fatal — errors are silently ignored.
pub fn revoke_cert(domain: &str, certs_dir: &Path) {
    let _ = std::fs::remove_file(certs_dir.join(format!("{}.pem", domain)));
    let _ = std::fs::remove_file(certs_dir.join(format!("{}-key.pem", domain)));
}

// ── Progress tracking ─────────────────────────────────────────────────────────

/// Per-site SSL issuance progress. Key = site id.
pub type SslTasks = Arc<Mutex<std::collections::HashMap<String, AsyncTask>>>;

pub fn new_ssl_tasks() -> SslTasks {
    Arc::new(Mutex::new(std::collections::HashMap::new()))
}

pub fn set_task(tasks: &SslTasks, id: &str, state: &str, message: &str, error: Option<String>) {
    tasks.lock().insert(id.to_string(), AsyncTask {
        state: state.into(),
        message: message.into(),
        error,
    });
}
