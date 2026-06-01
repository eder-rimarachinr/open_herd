use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};

const NGINX_VERSION: &str = "1.26.3";
const NGINX_URL:     &str = "https://nginx.org/download/nginx-1.26.3.zip";
const NGINX_SHA256:  &str = "39ca13277b361910f9e463a7e958e11566f7ede8a6f0df08a21b659ca92f3662";

// ── Progress ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadProgress {
    pub state:   String,
    pub message: String,
    pub percent: u8,
    pub error:   Option<String>,
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
        Arc::new(Self { nginx: Mutex::new(None), php: Mutex::new(std::collections::HashMap::new()) })
    }
}

// ── HTTP client ───────────────────────────────────────────────────────────────

fn make_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .user_agent("open-herd/0.1")
        .timeout(std::time::Duration::from_secs(30))
        .build().unwrap_or_default()
}

fn url_exists(client: &reqwest::blocking::Client, url: &str) -> bool {
    client.head(url).send().map(|r| r.status().is_success()).unwrap_or(false)
}

// ── SHA-256 ───────────────────────────────────────────────────────────────────

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop { let n = file.read(&mut buf)?; if n == 0 { break; } hasher.update(&buf[..n]); }
    Ok(hex::encode(hasher.finalize()))
}

fn verify_sha256(path: &Path, expected: &str) -> anyhow::Result<()> {
    let actual = sha256_file(path)?;
    if actual != expected {
        let _ = std::fs::remove_file(path);
        return Err(anyhow::anyhow!("SHA-256 mismatch — download may be corrupted.\n  expected: {}\n  actual:   {}", expected, actual));
    }
    Ok(())
}

// ── Known PHP hashes ──────────────────────────────────────────────────────────

fn known_php_hash(filename: &str) -> Option<&'static str> {
    match filename {
        "php-7.4.33-nts-Win32-vc15-x64.zip" => Some("14ae3250d4447c8ccfc4c45a70d90adfbcd61e728d85f0be56a7ddf8f9c8aace"),
        "php-8.0.30-nts-Win32-vs16-x64.zip" => Some("dfb70498ffa2c617f2f655a155564697e3c9cca41709938fd1a5997d1d5b0785"),
        "php-8.1.34-nts-Win32-vs16-x64.zip" => Some("9cfe246cb144076c16f5913a3ef88a474c3dd7e60f0f0c8bb95faf68674016cc"),
        "php-8.2.31-nts-Win32-vs16-x64.zip" => Some("941bd3b87683eb16d5ffa7f5997de9a5e07ef08ba875048e5c4d95b4fc778162"),
        "php-8.3.31-nts-Win32-vs16-x64.zip" => Some("389c1327d325f6b6b3b892a5b2e1484ca5b5df775b6c4ddf5d1b5dc3b34ac761"),
        "php-8.4.21-nts-Win32-vs17-x64.zip" => Some("2cb57d0d3a17b1248c6a53b600719d4b051e1c374373404d5031409c0725031d"),
        "php-8.5.6-nts-Win32-vs17-x64.zip"  => Some("e25cc9400a7d176f18074f677ef0159d6b04aecfb255c924d808b7144075092f"),
        _ => None,
    }
}

// ── Nginx download ────────────────────────────────────────────────────────────

pub fn download_nginx(dest_dir: &Path, progress: Arc<DownloadState>) {
    let dest_dir = dest_dir.to_path_buf();
    std::thread::spawn(move || {
        set_nginx(&progress, "downloading", &format!("Downloading nginx {}…", NGINX_VERSION), 5);
        match fetch_zip(NGINX_URL, &dest_dir, &format!("nginx-{}/", NGINX_VERSION), Some(NGINX_SHA256), &progress, |p, pct, msg| {
            set_nginx(p, "downloading", msg, pct);
        }) {
            Ok(())  => set_nginx(&progress, "done", "Nginx ready", 100),
            Err(e)  => *progress.nginx.lock() = Some(DownloadProgress::error(&e.to_string())),
        }
    });
}

fn set_nginx(progress: &Arc<DownloadState>, state: &str, msg: &str, pct: u8) {
    *progress.nginx.lock() = Some(DownloadProgress { state: state.into(), message: msg.into(), percent: pct, error: None });
}

// ── PHP download ──────────────────────────────────────────────────────────────

pub struct PhpRelease { pub major: String, pub version: String, pub url: String }

pub fn php_releases() -> Vec<PhpRelease> {
    vec![
        r("8.5","8.5.6"), r("8.4","8.4.21"), r("8.3","8.3.31"),
        r("8.2","8.2.31"), r("8.1","8.1.34"), r("8.0","8.0.30"), r("7.4","7.4.33"),
    ]
}
fn r(major: &str, version: &str) -> PhpRelease { PhpRelease { major: major.into(), version: version.into(), url: String::new() } }

fn compiler_variants(major: &str) -> &'static [&'static str] {
    match major {
        "8.5" => &["vs17"], "8.4" => &["vs17","vs16"],
        "8.3"|"8.2"|"8.1"|"8.0" => &["vs16","vs17"],
        "7.4" => &["vc15"], _ => &["vs16","vs17","vc15"],
    }
}

fn resolve_php_url(client: &reqwest::blocking::Client, major: &str, version: &str) -> Option<(String, String)> {
    let base = "https://windows.php.net/downloads/releases";
    let arc  = "https://windows.php.net/downloads/releases/archives";
    for vs in compiler_variants(major) {
        let f = format!("php-{}-nts-Win32-{}-x64.zip", version, vs);
        if url_exists(client, &format!("{}/{}", base, f)) { return Some((format!("{}/{}", base, f), f)); }
        if url_exists(client, &format!("{}/{}", arc, f))  { return Some((format!("{}/{}", arc, f), f)); }
    }
    None
}

pub fn download_php(major: &str, php_dir: &Path, progress: Arc<DownloadState>) {
    let major   = major.to_string();
    let php_dir = php_dir.to_path_buf();
    let release = match php_releases().into_iter().find(|r| r.major == major) {
        Some(r) => r,
        None => { set_php(&progress, &major, "error", &format!("Unknown PHP version: {}", major), 0); return; }
    };
    std::thread::spawn(move || {
        let client = make_client();
        let dest   = php_dir.join(&release.major);
        std::fs::create_dir_all(&dest).ok();
        set_php(&progress, &major, "downloading", &format!("Locating PHP {} on windows.php.net…", release.version), 3);
        let (url, filename) = match resolve_php_url(&client, &release.major, &release.version) {
            Some(p) => p,
            None => { set_php(&progress, &major, "error", &format!("PHP {} not found on windows.php.net", release.version), 0); return; }
        };
        let expected_hash = known_php_hash(&filename);
        set_php(&progress, &major, "downloading", &format!("Downloading PHP {}…", release.version), 8);
        let major2 = major.clone();
        let result = fetch_zip(&url, &dest, "", expected_hash, &progress, move |p, pct, msg| {
            set_php(p, &major2, "downloading", msg, pct);
        });
        match result {
            Ok(()) => { set_php(&progress, &major, "configuring", "Configuring php.ini…", 92); configure_ini(&dest); set_php(&progress, &major, "done", &format!("PHP {} installed", release.version), 100); }
            Err(e) => set_php(&progress, &major, "error", &e.to_string(), 0),
        }
    });
}

fn set_php(progress: &Arc<DownloadState>, major: &str, state: &str, msg: &str, pct: u8) {
    progress.php.lock().insert(major.to_string(), DownloadProgress { state: state.into(), message: msg.into(), percent: pct, error: None });
}

fn configure_ini(dest_dir: &Path) {
    let src = dest_dir.join("php.ini-production");
    let dst = dest_dir.join("php.ini");
    if dst.exists() { return; }
    let content = if src.exists() { std::fs::read_to_string(&src).unwrap_or_default() } else { minimal_php_ini().to_string() };
    let mut out = content;
    for ext in &["curl","mbstring","openssl","pdo_mysql","pdo_sqlite","fileinfo","intl","zip"] {
        out = out.replace(&format!(";extension={}", ext), &format!("extension={}", ext));
    }
    out = out.replace(";extension_dir = \"ext\"", "extension_dir = \"ext\"");
    out = out.replace(";cgi.force_redirect = 1", "cgi.force_redirect = 0");
    out = out.replace(";cgi.fix_pathinfo=1", "cgi.fix_pathinfo=1");
    let _ = std::fs::write(dst, out);
}

fn minimal_php_ini() -> &'static str {
    "extension_dir = \"ext\"\ncgi.force_redirect = 0\ncgi.fix_pathinfo = 1\ndisplay_errors = Off\nlog_errors = On\nextension=curl\nextension=mbstring\nextension=openssl\nextension=pdo_mysql\nextension=pdo_sqlite\nextension=fileinfo\n"
}

// ── Shared fetch + extract ────────────────────────────────────────────────────

fn fetch_zip<F>(url: &str, dest_dir: &PathBuf, strip_prefix: &str, expected_sha256: Option<&str>, progress: &Arc<DownloadState>, mut on_progress: F) -> anyhow::Result<()>
where F: FnMut(&Arc<DownloadState>, u8, &str) {
    use std::io::Read;
    std::fs::create_dir_all(dest_dir)?;
    let client   = make_client();
    let mut resp = client.get(url).send()?;
    if !resp.status().is_success() { return Err(anyhow::anyhow!("HTTP {} for {}", resp.status(), url)); }
    let total    = resp.content_length().unwrap_or(0);
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
            let pct = (8 + (downloaded * 60 / total).min(60)) as u8;
            on_progress(progress, pct, &format!("Downloading… {:.1} / {:.1} MB", downloaded as f64 / 1_048_576.0, total as f64 / 1_048_576.0));
        }
    }
    drop(file);
    if let Some(expected) = expected_sha256 { on_progress(progress, 72, "Verifying integrity…"); verify_sha256(&zip_path, expected)?; }
    on_progress(progress, 73, "Extracting…");
    let file = std::fs::File::open(&zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let raw_name  = entry.name().to_string();
        let name      = if strip_prefix.is_empty() { raw_name.as_str() } else { raw_name.strip_prefix(strip_prefix).unwrap_or(&raw_name) };
        if name.is_empty() { continue; }
        if std::path::Path::new(name).components().any(|c| c == std::path::Component::ParentDir) { continue; }
        let out_path = dest_dir.join(name);
        if entry.is_dir() { std::fs::create_dir_all(&out_path)?; }
        else {
            if let Some(parent) = out_path.parent() { std::fs::create_dir_all(parent)?; }
            let mut out = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    let _ = std::fs::remove_file(&zip_path);
    Ok(())
}
