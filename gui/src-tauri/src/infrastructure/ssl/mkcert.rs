use std::path::{Path, PathBuf};
use std::process::Command;
use sha2::{Digest, Sha256};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

const MKCERT_VERSION: &str = "v1.4.4";

#[cfg(target_os = "windows")]
const MKCERT_URL: &str = "https://github.com/FiloSottile/mkcert/releases/download/v1.4.4/mkcert-v1.4.4-windows-amd64.exe";
#[cfg(not(target_os = "windows"))]
const MKCERT_URL: &str = "https://github.com/FiloSottile/mkcert/releases/download/v1.4.4/mkcert-v1.4.4-linux-amd64";

// SHA-256 pinned per platform. mkcert is downloaded and then executed (it installs
// a root CA into the system trust store), so we MUST verify integrity before writing
// it to disk — a MITM or compromised release would otherwise run arbitrary code.
#[cfg(target_os = "windows")]
const MKCERT_SHA256: &str = "d2660b50a9ed59eada480750561c96abc2ed4c9a38c6a24d93e30e0977631398";
#[cfg(not(target_os = "windows"))]
const MKCERT_SHA256: &str = "6d31c65b03972c6dc4a14ab429f2928300518b26503f58723e532d1b0a3bbb52";

pub fn mkcert_path(base_dir: &Path) -> PathBuf {
    #[cfg(target_os = "windows")] return base_dir.join("mkcert.exe");
    #[cfg(not(target_os = "windows"))] return base_dir.join("mkcert");
}

pub fn ensure_mkcert(base_dir: &Path) -> Result<PathBuf, String> {
    let path = mkcert_path(base_dir);
    if path.exists() { return Ok(path); }
    let client = reqwest::blocking::Client::builder().user_agent("open-herd/0.1").timeout(std::time::Duration::from_secs(120)).build().map_err(|e| e.to_string())?;
    let resp   = client.get(MKCERT_URL).send().map_err(|e| format!("Failed to download mkcert {}: {}", MKCERT_VERSION, e))?;
    if !resp.status().is_success() { return Err(format!("mkcert download returned HTTP {}", resp.status())); }
    let bytes  = resp.bytes().map_err(|e| e.to_string())?;
    let actual = hex::encode(Sha256::digest(&bytes));
    if actual != MKCERT_SHA256 {
        return Err(format!(
            "mkcert SHA-256 mismatch — refusing to run an untrusted binary.\n  expected: {}\n  actual:   {}",
            MKCERT_SHA256, actual
        ));
    }
    std::fs::write(&path, &bytes).map_err(|e| format!("Failed to save mkcert: {}", e))?;
    #[cfg(not(target_os = "windows"))] {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).map_err(|e| format!("Failed to set mkcert permissions: {}", e))?;
    }
    Ok(path)
}

pub fn install_ca(mkcert: &Path) -> Result<(), String> {
    #[allow(unused_mut)] let mut cmd = Command::new(mkcert); cmd.arg("-install");
    #[cfg(target_os = "windows")] cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd.output().map_err(|e| format!("Failed to run mkcert -install: {}", e))?;
    if !output.status.success() { return Err(format!("mkcert -install failed: {}", String::from_utf8_lossy(&output.stderr).trim())); }
    Ok(())
}

pub struct CertPaths { pub cert: PathBuf, pub key: PathBuf }

pub fn issue_cert(mkcert: &Path, domain: &str, certs_dir: &Path) -> Result<CertPaths, String> {
    std::fs::create_dir_all(certs_dir).map_err(|e| format!("Failed to create certs dir: {}", e))?;
    let cert   = certs_dir.join(format!("{}.pem", domain));
    let key    = certs_dir.join(format!("{}-key.pem", domain));
    #[allow(unused_mut)] let mut cmd = Command::new(mkcert);
    cmd.args(["-cert-file", &cert.to_string_lossy().to_string(), "-key-file", &key.to_string_lossy().to_string(), domain]);
    #[cfg(target_os = "windows")] cmd.creation_flags(CREATE_NO_WINDOW);
    let output = cmd.output().map_err(|e| format!("Failed to run mkcert: {}", e))?;
    if !output.status.success() { return Err(format!("mkcert cert issuance failed: {}", String::from_utf8_lossy(&output.stderr).trim())); }
    Ok(CertPaths { cert, key })
}

pub fn revoke_cert(domain: &str, certs_dir: &Path) {
    let _ = std::fs::remove_file(certs_dir.join(format!("{}.pem", domain)));
    let _ = std::fs::remove_file(certs_dir.join(format!("{}-key.pem", domain)));
}
