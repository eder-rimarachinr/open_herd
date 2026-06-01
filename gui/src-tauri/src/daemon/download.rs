use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::Mutex;

const NGINX_VERSION: &str = "1.26.3";
const NGINX_URL: &str = "https://nginx.org/download/nginx-1.26.3.zip";

// ── Progress ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadProgress {
    pub state: String, // pending | downloading | extracting | configuring | done | error
    pub message: String,
    pub percent: u8,
    pub error: Option<String>,
}

impl DownloadProgress {
    pub fn error(e: &str) -> Self {
        Self { state: "error".into(), message: e.into(), percent: 0, error: Some(e.into()) }
    }
}

pub struct DownloadState {
    pub nginx: Mutex<Option<DownloadProgress>>,
    pub php:   Mutex<std::collections::HashMap<String, DownloadProgress>>,
}

impl DownloadState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            nginx: Mutex::new(None),
            php:   Mutex::new(std::collections::HashMap::new()),
        })
    }
}

// ── HTTP client (shared) ──────────────────────────────────────────────────────

fn make_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent("open-herd/0.1 (https://github.com/open-herd)")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default()
}

fn url_exists(client: &reqwest::blocking::Client, url: &str) -> bool {
    client.head(url).send().map(|r| r.status().is_success()).unwrap_or(false)
}

// ── Nginx ─────────────────────────────────────────────────────────────────────

pub fn download_nginx(dest_dir: &Path, progress: Arc<DownloadState>) {
    let dest_dir = dest_dir.to_path_buf();
    std::thread::spawn(move || {
        set_nginx(&progress, "downloading", &format!("Downloading nginx {}…", NGINX_VERSION), 5);
        match fetch_zip(NGINX_URL, &dest_dir, &format!("nginx-{}/", NGINX_VERSION), &progress, |p, pct, msg| {
            set_nginx(p, "downloading", msg, pct);
        }) {
            Ok(()) => set_nginx(&progress, "done", "Nginx ready", 100),
            Err(e) => *progress.nginx.lock() = Some(DownloadProgress::error(&e.to_string())),
        }
    });
}

fn set_nginx(progress: &Arc<DownloadState>, state: &str, msg: &str, pct: u8) {
    *progress.nginx.lock() = Some(DownloadProgress {
        state: state.into(), message: msg.into(), percent: pct, error: None,
    });
}

// ── PHP ───────────────────────────────────────────────────────────────────────

pub struct PhpRelease {
    pub major:   String,
    pub version: String,
    pub url:     String,
}

/// Returns the known catalog — matches windows.php.net current releases
pub fn php_releases() -> Vec<PhpRelease> {
    vec![
        r("8.5", "8.5.6"),
        r("8.4", "8.4.21"),
        r("8.3", "8.3.31"),
        r("8.2", "8.2.31"),
        r("8.1", "8.1.34"),
        r("8.0", "8.0.30"),
        r("7.4", "7.4.33"),
    ]
}

fn r(major: &str, version: &str) -> PhpRelease {
    PhpRelease { major: major.into(), version: version.into(), url: String::new() }
}

/// Compiler variants per major — matches Go's compilerOrder
fn compiler_variants(major: &str) -> &'static [&'static str] {
    match major {
        "8.5" => &["vs17"],
        "8.4" => &["vs17", "vs16"],
        "8.3" | "8.2" | "8.1" | "8.0" => &["vs16", "vs17"],
        "7.4" => &["vc15"],
        _     => &["vs16", "vs17", "vc15"],
    }
}

/// Try main releases/ then archives/ for each compiler variant — same as Go's resolveDownloadURL
fn resolve_php_url(client: &reqwest::blocking::Client, major: &str, version: &str) -> Option<String> {
    let base     = "https://windows.php.net/downloads/releases";
    let archives = "https://windows.php.net/downloads/releases/archives";

    for vs in compiler_variants(major) {
        let filename = format!("php-{}-nts-Win32-{}-x64.zip", version, vs);
        let url = format!("{}/{}", base, filename);
        if url_exists(client, &url) { return Some(url); }
        let archive_url = format!("{}/{}", archives, filename);
        if url_exists(client, &archive_url) { return Some(archive_url); }
    }
    None
}

pub fn download_php(major: &str, php_dir: &Path, progress: Arc<DownloadState>) {
    let major    = major.to_string();
    let php_dir  = php_dir.to_path_buf();

    // Find the release entry
    let release = match php_releases().into_iter().find(|r| r.major == major) {
        Some(r) => r,
        None => {
            set_php(&progress, &major, "error", &format!("Unknown PHP version: {}", major), 0);
            return;
        }
    };

    std::thread::spawn(move || {
        let client  = make_client();
        let dest    = php_dir.join(&release.major);
        std::fs::create_dir_all(&dest).ok();

        // 1. Resolve download URL (HEAD requests like Go does)
        set_php(&progress, &major, "downloading",
            &format!("Locating PHP {} on windows.php.net…", release.version), 3);

        let url = match resolve_php_url(&client, &release.major, &release.version) {
            Some(u) => u,
            None => {
                let msg = format!(
                    "PHP {} not found on windows.php.net. Check https://windows.php.net/download",
                    release.version
                );
                set_php(&progress, &major, "error", &msg, 0);
                return;
            }
        };

        // 2. Download
        set_php(&progress, &major, "downloading",
            &format!("Downloading PHP {}…", release.version), 8);

        let major2 = major.clone();
        let result = fetch_zip(&url, &dest, "", &progress, move |p, pct, msg| {
            set_php(p, &major2, "downloading", msg, pct);
        });

        match result {
            Ok(()) => {
                // 3. Configure php.ini (copy from php.ini-production, enable common extensions)
                set_php(&progress, &major, "configuring", "Configuring php.ini…", 92);
                configure_ini(&dest);
                set_php(&progress, &major, "done",
                    &format!("PHP {} installed", release.version), 100);
            }
            Err(e) => set_php(&progress, &major, "error", &e.to_string(), 0),
        }
    });
}

fn set_php(progress: &Arc<DownloadState>, major: &str, state: &str, msg: &str, pct: u8) {
    progress.php.lock().insert(major.to_string(), DownloadProgress {
        state: state.into(), message: msg.into(), percent: pct, error: None,
    });
}

/// Copy php.ini-production → php.ini and enable common extensions (matches Go's configureINI)
fn configure_ini(dest_dir: &Path) {
    let src = dest_dir.join("php.ini-production");
    let dst = dest_dir.join("php.ini");
    if dst.exists() { return; }

    let content = if src.exists() {
        std::fs::read_to_string(&src).unwrap_or_default()
    } else {
        minimal_php_ini().to_string()
    };

    // Enable the most commonly needed extensions (same as Go)
    let mut out = content;
    for ext in &["curl", "mbstring", "openssl", "pdo_mysql", "pdo_sqlite", "fileinfo", "intl", "zip"] {
        out = out.replace(&format!(";extension={}", ext), &format!("extension={}", ext));
    }
    out = out.replace(";extension_dir = \"ext\"", "extension_dir = \"ext\"");
    // Enable CGI
    out = out.replace(";cgi.force_redirect = 1", "cgi.force_redirect = 0");
    out = out.replace(";cgi.fix_pathinfo=1", "cgi.fix_pathinfo=1");

    let _ = std::fs::write(dst, out);
}

fn minimal_php_ini() -> &'static str {
    r#"extension_dir = "ext"
cgi.force_redirect = 0
cgi.fix_pathinfo = 1
display_errors = Off
log_errors = On
extension=curl
extension=mbstring
extension=openssl
extension=pdo_mysql
extension=pdo_sqlite
extension=fileinfo
"#
}

// ── Shared download + extract ─────────────────────────────────────────────────

fn fetch_zip<F>(
    url: &str,
    dest_dir: &PathBuf,
    strip_prefix: &str,
    progress: &Arc<DownloadState>,
    mut on_progress: F,
) -> anyhow::Result<()>
where
    F: FnMut(&Arc<DownloadState>, u8, &str),
{
    use std::io::Read;

    std::fs::create_dir_all(dest_dir)?;

    let client = make_client();
    let mut resp = client.get(url).send()?;
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("HTTP {} for {}", resp.status(), url));
    }

    let total = resp.content_length().unwrap_or(0);
    let zip_path = dest_dir.join("_download.zip");
    let mut file = std::fs::File::create(&zip_path)?;
    let mut downloaded = 0u64;
    let mut buf = [0u8; 32768];

    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 { break; }
        file.write_all(&buf[..n])?;
        downloaded += n as u64;
        if total > 0 {
            // 8..72 range — matches Go: pct = 8 + int(downloaded*60/total)
            let pct = (8 + (downloaded * 60 / total).min(60)) as u8;
            let msg = format!("Downloading… {:.1} / {:.1} MB",
                downloaded as f64 / 1_048_576.0,
                total as f64 / 1_048_576.0);
            on_progress(progress, pct, &msg);
        }
    }
    drop(file);

    on_progress(progress, 72, "Extracting…");

    // Extract — with zip-slip guard (same as Go)
    let file = std::fs::File::open(&zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let raw_name = entry.name().to_string();
        let name = if strip_prefix.is_empty() {
            raw_name.as_str()
        } else {
            raw_name.strip_prefix(strip_prefix).unwrap_or(&raw_name)
        };
        if name.is_empty() { continue; }

        let out_path = dest_dir.join(name);

        // Zip-slip guard
        let canonical_dest = dest_dir.canonicalize().unwrap_or_else(|_| dest_dir.clone());
        if let Ok(canonical_out) = out_path.parent().map(|p| p.to_path_buf()).unwrap_or_default().canonicalize() {
            if !canonical_out.starts_with(&canonical_dest) { continue; }
        }

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }

    let _ = std::fs::remove_file(&zip_path);
    Ok(())
}
