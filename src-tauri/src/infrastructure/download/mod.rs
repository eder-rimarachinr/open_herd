use std::time::Duration;
use std::path::Path;
use std::sync::Arc;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};

use crate::domain::ports::{download::DownloadProgress, task::TaskState};
#[cfg(windows)]
use crate::infrastructure::php::catalog::php_line;
#[cfg(not(windows))]
use crate::infrastructure::php::catalog::{static_php_line, STATIC_PHP_BASE};

pub mod node;
pub mod tracker;

const NGINX_VERSION: &str = "1.26.3";
const NGINX_URL:     &str = "https://nginx.org/download/nginx-1.26.3.zip";
const NGINX_SHA256:  &str = "39ca13277b361910f9e463a7e958e11566f7ede8a6f0df08a21b659ca92f3662";

// ── Progress ──────────────────────────────────────────────────────────────────

pub struct DownloadState {
    pub nginx: Mutex<Option<DownloadProgress>>,
    pub php:   Mutex<std::collections::HashMap<String, DownloadProgress>>,
    /// Keyed by exact Node version (`22.19.0`).
    pub node:  Mutex<std::collections::HashMap<String, DownloadProgress>>,
}

impl DownloadState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            nginx: Mutex::new(None),
            php: Mutex::new(std::collections::HashMap::new()),
            node: Mutex::new(std::collections::HashMap::new()),
        })
    }
}

// ── HTTP client ───────────────────────────────────────────────────────────────

/// Async client for every download. In async reqwest `timeout()` is a *total*
/// deadline and would cut off a slow-but-steady 30 MB download, so only the
/// connect phase and each individual read are bounded: a stalled connection
/// fails after 30 s, a slow one keeps going.
pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("open-herd/0.1")
        .connect_timeout(Duration::from_secs(30))
        .read_timeout(Duration::from_secs(30))
        .build().unwrap_or_default()
}

#[cfg(windows)]
async fn url_exists(client: &reqwest::Client, url: &str) -> bool {
    client.head(url).send().await.map(|r| r.status().is_success()).unwrap_or(false)
}

// ── SHA-256 ───────────────────────────────────────────────────────────────────

fn sha256_file(path: &Path) -> anyhow::Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop { let n = file.read(&mut buf)?; if n == 0 { break; } hasher.update(&buf[..n]); }
    Ok(format!("{:x}", hasher.finalize()))
}

fn verify_sha256(path: &Path, expected: &str) -> anyhow::Result<()> {
    let actual = sha256_file(path)?;
    if actual != expected {
        let _ = std::fs::remove_file(path);
        return Err(anyhow::anyhow!("SHA-256 mismatch — download may be corrupted.\n  expected: {}\n  actual:   {}", expected, actual));
    }
    Ok(())
}

// ── Nginx download ────────────────────────────────────────────────────────────

/// Starts the download as a task on the current Tokio runtime and returns at
/// once; progress is published through `progress`.
pub fn download_nginx(dest_dir: &Path, progress: Arc<DownloadState>) {
    let dest_dir = dest_dir.to_path_buf();
    tokio::spawn(async move {
        set_nginx(&progress, TaskState::Downloading, &format!("Downloading nginx {}…", NGINX_VERSION), 5);
        match fetch_zip(NGINX_URL, &dest_dir, &format!("nginx-{}/", NGINX_VERSION), Some(NGINX_SHA256), &progress, |p, pct, msg| {
            set_nginx(p, TaskState::Downloading, msg, pct);
        }).await {
            Ok(())  => set_nginx(&progress, TaskState::Done, "Nginx ready", 100),
            Err(e)  => *progress.nginx.lock() = Some(DownloadProgress::error(&e.to_string())),
        }
    });
}

fn set_nginx(progress: &Arc<DownloadState>, state: TaskState, msg: &str, pct: u8) {
    *progress.nginx.lock() = Some(DownloadProgress { state, message: msg.into(), percent: pct, error: None });
}

// ── PHP download ──────────────────────────────────────────────────────────────

/// Tries the current releases folder first, then the archive (where every
/// end-of-life build lives). `windows.php.net/downloads` redirects to these.
#[cfg(windows)]
async fn resolve_php_url(client: &reqwest::Client, filename: &str) -> Option<String> {
    const BASES: [&str; 2] = [
        "https://windows.php.net/downloads/releases",
        "https://windows.php.net/downloads/releases/archives",
    ];
    for base in BASES {
        let url = format!("{base}/{filename}");
        if url_exists(client, &url).await { return Some(url); }
    }
    None
}

/// Starts the download as a task on the current Tokio runtime and returns at
/// once; progress is published through `progress`.
#[cfg(windows)]
pub fn download_php(major: &str, php_dir: &Path, progress: Arc<DownloadState>) {
    let major   = major.to_string();
    let php_dir = php_dir.to_path_buf();
    let Some(release) = php_line(&major) else {
        set_php(&progress, &major, TaskState::Error, &format!("Unknown PHP version: {}", major), 0);
        return;
    };
    tokio::spawn(async move {
        let client   = http_client();
        let dest     = php_dir.join(release.major);
        let filename = release.filename();
        set_php(&progress, &major, TaskState::Downloading, &format!("Locating PHP {} on windows.php.net…", release.version), 3);
        let Some(url) = resolve_php_url(&client, &filename).await else {
            set_php(&progress, &major, TaskState::Error, &format!("PHP {} not found on windows.php.net", release.version), 0);
            return;
        };
        set_php(&progress, &major, TaskState::Downloading, &format!("Downloading PHP {}…", release.version), 8);
        let major2 = major.clone();
        // Every line carries a pinned SHA-256: an unverified build is never installed.
        let result = fetch_zip(&url, &dest, "", Some(release.sha256), &progress, move |p, pct, msg| {
            set_php(p, &major2, TaskState::Downloading, msg, pct);
        }).await;
        if let Err(e) = result {
            set_php(&progress, &major, TaskState::Error, &e.to_string(), 0);
            return;
        }
        set_php(&progress, &major, TaskState::Configuring, "Configuring php.ini…", 92);
        let dest2 = dest.clone();
        let legacy = release.legacy_extension_names();
        let _ = tokio::task::spawn_blocking(move || configure_ini(&dest2, legacy)).await;
        let starts = tokio::task::spawn_blocking(move || php_starts(&dest)).await.unwrap_or(PhpStart::Ok);
        match starts {
            PhpStart::Ok => set_php(&progress, &major, TaskState::Done, &format!("PHP {} installed", release.version), 100),
            PhpStart::MissingRuntime => {
                let rt = release.runtime;
                let msg = format!(
                    "PHP {} is installed but cannot start: Windows is missing the {}. Install it from {} and press Rescan.",
                    release.version, rt.label(), rt.download_url(),
                );
                set_php(&progress, &major, TaskState::Error, &msg, 0);
            }
            PhpStart::Failed(why) => {
                let msg = format!("PHP {} is installed but `php -v` failed: {why}", release.version);
                set_php(&progress, &major, TaskState::Error, &msg, 0);
            }
        }
    });
}

/// Linux: the static-php-cli `php-fpm` and `php` tarballs (x86_64), each
/// checked against its pinned SHA-256, into `php_dir/<major>`. Starts the
/// download as a task on the current Tokio runtime and returns at once.
#[cfg(not(windows))]
pub fn download_php(major: &str, php_dir: &Path, progress: Arc<DownloadState>) {
    let major   = major.to_string();
    let php_dir = php_dir.to_path_buf();
    let Some(release) = static_php_line(&major) else {
        set_php(&progress, &major, TaskState::Error, &format!("PHP {major} has no Linux build (static builds cover PHP 8.0 and newer)"), 0);
        return;
    };
    if std::env::consts::ARCH != "x86_64" {
        set_php(&progress, &major, TaskState::Error, &format!("PHP downloads need an x86_64 machine (this one is {})", std::env::consts::ARCH), 0);
        return;
    }
    tokio::spawn(async move {
        let dest = php_dir.join(release.major);
        // php-fpm first: the detector only counts the install once `php` is there too.
        let parts = [("fpm", "php-fpm", release.fpm_sha256, 0u8), ("cli", "php CLI", release.cli_sha256, 44u8)];
        for (sapi, label, sha, base) in parts {
            let url = format!("{STATIC_PHP_BASE}/{}", release.filename(sapi));
            set_php(&progress, &major, TaskState::Downloading, &format!("Downloading PHP {} ({label})…", release.version), base + 4);
            let major2 = major.clone();
            let result = fetch_tar_gz(&url, &dest, "", Some(sha), &progress, move |p, pct, msg| {
                set_php(p, &major2, TaskState::Downloading, &format!("{label}: {msg}"), base + pct / 2);
            }).await;
            if let Err(e) = result {
                set_php(&progress, &major, TaskState::Error, &e.to_string(), 0);
                return;
            }
        }
        set_php(&progress, &major, TaskState::Configuring, "Configuring php.ini…", 92);
        let dest2 = dest.clone();
        let _ = tokio::task::spawn_blocking(move || configure_static_ini(&dest2)).await;
        let starts = tokio::task::spawn_blocking(move || php_starts(&dest))
            .await.unwrap_or_else(|e| PhpStart::Failed(e.to_string()));
        match starts {
            PhpStart::Ok => set_php(&progress, &major, TaskState::Done, &format!("PHP {} installed", release.version), 100),
            PhpStart::Failed(why) => {
                let msg = format!("PHP {} is installed but `php -v` failed: {why}", release.version);
                set_php(&progress, &major, TaskState::Error, &msg, 0);
            }
        }
    });
}

/// php.ini for a static build: its extensions are compiled in, so there is
/// nothing to enable; only sane development limits. An existing file (user
/// edits) is kept.
#[cfg(not(windows))]
fn configure_static_ini(dest_dir: &Path) {
    let dst = dest_dir.join("php.ini");
    if dst.exists() { return; }
    let _ = std::fs::write(dst, "\
; Open Herd: static PHP build (static-php-cli). Extensions are compiled in;
; `php -m` lists them.
memory_limit = 512M
upload_max_filesize = 64M
post_max_size = 64M
max_execution_time = 120
display_errors = Off
log_errors = On
cgi.fix_pathinfo = 1
");
}

#[derive(Debug, PartialEq)]
enum PhpStart {
    Ok,
    #[cfg(windows)]
    MissingRuntime,
    Failed(String),
}

/// Windows' STATUS_DLL_NOT_FOUND / STATUS_INVALID_IMAGE_FORMAT-style loader
/// failures, as a process exit code: a Visual C++ runtime DLL is missing.
#[cfg(windows)]
fn is_missing_dll(code: i32) -> bool {
    const STATUS_DLL_NOT_FOUND: u32 = 0xC000_0135;
    const STATUS_ENTRYPOINT_NOT_FOUND: u32 = 0xC000_0139;
    const STATUS_INVALID_IMAGE_FORMAT: u32 = 0xC000_007B;
    [STATUS_DLL_NOT_FOUND, STATUS_ENTRYPOINT_NOT_FOUND, STATUS_INVALID_IMAGE_FORMAT].contains(&(code as u32))
}

/// Runs `php -v` from a fresh install: an old Windows build on a machine
/// without its Visual C++ runtime extracts fine and then never starts, and a
/// Linux binary may not run on this machine at all.
fn php_starts(dir: &Path) -> PhpStart {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(dir.join(if cfg!(windows) { "php.exe" } else { "php" }));
    cmd.arg("-v");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    match cmd.output() {
        Ok(o) if o.status.success() => PhpStart::Ok,
        #[cfg(windows)]
        Ok(o) if o.status.code().is_some_and(is_missing_dll) => PhpStart::MissingRuntime,
        Ok(o) => PhpStart::Failed(format!("exit code {:?}: {}", o.status.code(), String::from_utf8_lossy(&o.stderr).trim())),
        Err(e) => PhpStart::Failed(e.to_string()),
    }
}

fn set_php(progress: &Arc<DownloadState>, major: &str, state: TaskState, msg: &str, pct: u8) {
    progress.php.lock().insert(major.to_string(), DownloadProgress { state, message: msg.into(), percent: pct, error: None });
}

/// `legacy_names`: PHP before 7.2 spells extensions `php_<name>.dll`.
#[cfg(windows)]
fn configure_ini(dest_dir: &Path, legacy_names: bool) {
    let src = dest_dir.join("php.ini-production");
    let dst = dest_dir.join("php.ini");
    if dst.exists() { return; }
    let content = if src.exists() { std::fs::read_to_string(&src).unwrap_or_default() } else { minimal_php_ini().to_string() };
    let _ = std::fs::write(dst, configured_ini(content, legacy_names));
}

#[cfg(any(windows, test))]
fn configured_ini(content: String, legacy_names: bool) -> String {
    let mut out = content;
    for ext in &["curl","mbstring","openssl","pdo_mysql","pdo_sqlite","fileinfo","intl","zip"] {
        let name = if legacy_names { format!("php_{ext}.dll") } else { ext.to_string() };
        out = out.replace(&format!(";extension={name}"), &format!("extension={name}"));
    }
    // 7.4+ writes `;extension_dir`, older php.ini files `; extension_dir`.
    out = out.replace(";extension_dir = \"ext\"", "extension_dir = \"ext\"");
    out = out.replace("; extension_dir = \"ext\"", "extension_dir = \"ext\"");
    out = out.replace(";cgi.force_redirect = 1", "cgi.force_redirect = 0");
    out = out.replace(";cgi.fix_pathinfo=1", "cgi.fix_pathinfo=1");
    out
}

#[cfg(windows)]
fn minimal_php_ini() -> &'static str {
    "extension_dir = \"ext\"\ncgi.force_redirect = 0\ncgi.fix_pathinfo = 1\ndisplay_errors = Off\nlog_errors = On\nextension=curl\nextension=mbstring\nextension=openssl\nextension=pdo_mysql\nextension=pdo_sqlite\nextension=fileinfo\n"
}

// ── Shared fetch + extract ────────────────────────────────────────────────────

/// Present in an install dir while its archive is being extracted. A dir that
/// still has it after a failure is half-installed and must not be treated as a
/// usable PHP / nginx (the next install attempt overwrites it).
pub const INSTALL_INCOMPLETE_MARKER: &str = ".install-incomplete";

pub fn is_install_complete(dir: &Path) -> bool {
    !dir.join(INSTALL_INCOMPLETE_MARKER).exists()
}

/// Unpacks an archive over a destination dir, stripping a top-level prefix.
type Extractor = fn(&Path, &Path, &str) -> anyhow::Result<()>;

async fn fetch_zip<F>(url: &str, dest_dir: &Path, strip_prefix: &str, expected_sha256: Option<&str>, progress: &Arc<DownloadState>, on_progress: F) -> anyhow::Result<()>
where F: FnMut(&Arc<DownloadState>, u8, &str) + Send {
    fetch_archive(url, dest_dir, "_download.zip", extract_zip, strip_prefix, expected_sha256, progress, on_progress).await
}

#[cfg(not(windows))]
async fn fetch_tar_gz<F>(url: &str, dest_dir: &Path, strip_prefix: &str, expected_sha256: Option<&str>, progress: &Arc<DownloadState>, on_progress: F) -> anyhow::Result<()>
where F: FnMut(&Arc<DownloadState>, u8, &str) + Send {
    fetch_archive(url, dest_dir, "_download.tar.gz", extract_tar_gz, strip_prefix, expected_sha256, progress, on_progress).await
}

#[allow(clippy::too_many_arguments)]
async fn fetch_archive<F>(url: &str, dest_dir: &Path, temp_name: &str, extract: Extractor, strip_prefix: &str, expected_sha256: Option<&str>, progress: &Arc<DownloadState>, on_progress: F) -> anyhow::Result<()>
where F: FnMut(&Arc<DownloadState>, u8, &str) + Send {
    tokio::fs::create_dir_all(dest_dir).await?;
    let archive_path = dest_dir.join(temp_name);
    let result = download_and_extract(url, &archive_path, dest_dir, extract, strip_prefix, expected_sha256, progress, on_progress).await;
    // The archive is only a temporary: never leave it behind, success or not.
    let _ = tokio::fs::remove_file(&archive_path).await;
    result
}

#[allow(clippy::too_many_arguments)]
async fn download_and_extract<F>(url: &str, zip_path: &Path, dest_dir: &Path, extract: Extractor, strip_prefix: &str, expected_sha256: Option<&str>, progress: &Arc<DownloadState>, mut on_progress: F) -> anyhow::Result<()>
where F: FnMut(&Arc<DownloadState>, u8, &str) + Send {
    use tokio::io::AsyncWriteExt;
    let mut resp = http_client().get(url).send().await?;
    if !resp.status().is_success() { return Err(anyhow::anyhow!("HTTP {} for {}", resp.status(), url)); }
    let total    = resp.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(zip_path).await?;
    let mut downloaded = 0u64;
    while let Some(chunk) = resp.chunk().await? {
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        if let Some(ratio) = (downloaded * 60).checked_div(total) {
            let pct = (8 + ratio.min(60)) as u8;
            on_progress(progress, pct, &format!("Downloading… {:.1} / {:.1} MB", downloaded as f64 / 1_048_576.0, total as f64 / 1_048_576.0));
        }
    }
    // tokio's File buffers writes: flush before the blocking code reopens it.
    file.flush().await?;
    drop(file);

    if expected_sha256.is_some() { on_progress(progress, 72, "Verifying integrity…"); }
    let (zip, dest, strip, expected) =
        (zip_path.to_path_buf(), dest_dir.to_path_buf(), strip_prefix.to_owned(), expected_sha256.map(str::to_owned));
    // Hashing and extracting ~30 MB is CPU/disk work: keep it off the async workers.
    on_progress(progress, 73, "Extracting…");
    tokio::task::spawn_blocking(move || {
        if let Some(expected) = expected { verify_sha256(&zip, &expected)?; }
        extract(&zip, &dest, &strip)
    }).await?
}

/// Extracts over `dest_dir` (keeping files the archive does not contain, e.g.
/// a user-edited php.ini or nginx `sites/`), flagged incomplete until it ends.
fn extract_zip(zip_path: &Path, dest_dir: &Path, strip_prefix: &str) -> anyhow::Result<()> {
    let marker = dest_dir.join(INSTALL_INCOMPLETE_MARKER);
    std::fs::write(&marker, "extraction in progress or interrupted\n")?;
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let raw_name  = entry.name().to_string();
        let name      = if strip_prefix.is_empty() { raw_name.as_str() } else { raw_name.strip_prefix(strip_prefix).unwrap_or(&raw_name) };
        if name.is_empty() { continue; }
        // Zip-slip defence: reject `..`, absolute paths and Windows drive prefixes.
        // `dest_dir.join(absolute)` would otherwise escape dest_dir entirely.
        use std::path::Component;
        if std::path::Path::new(name).components().any(|c|
            matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_))
        ) { continue; }
        let out_path = dest_dir.join(name);
        if entry.is_dir() { std::fs::create_dir_all(&out_path)?; }
        else {
            if let Some(parent) = out_path.parent() { std::fs::create_dir_all(parent)?; }
            let mut out = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    std::fs::remove_file(&marker)?;
    Ok(())
}

/// The `.tar.gz` counterpart of [`extract_zip`] (Linux PHP and Node), with the
/// same in-progress marker and path checks. Keeps symlinks and file modes.
#[cfg(not(windows))]
fn extract_tar_gz(archive_path: &Path, dest_dir: &Path, strip_prefix: &str) -> anyhow::Result<()> {
    use std::path::Component;
    let marker = dest_dir.join(INSTALL_INCOMPLETE_MARKER);
    std::fs::write(&marker, "extraction in progress or interrupted\n")?;
    let file = std::fs::File::open(archive_path)?;
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    let prefix = Path::new(strip_prefix.trim_end_matches('/'));
    for entry in archive.entries()? {
        let mut entry = entry?;
        let raw = entry.path()?.into_owned();
        let name = if strip_prefix.is_empty() { raw.as_path() } else { raw.strip_prefix(prefix).unwrap_or(&raw) };
        if name.as_os_str().is_empty() { continue; }
        if name.components().any(|c| matches!(c, Component::ParentDir | Component::RootDir | Component::Prefix(_))) { continue; }
        let out_path = dest_dir.join(name);
        if let Some(parent) = out_path.parent() { std::fs::create_dir_all(parent)?; }
        // Replace files instead of writing through them: a running binary
        // can't be overwritten in place, and an old symlink must not redirect the write.
        if out_path.symlink_metadata().is_ok_and(|m| !m.is_dir()) { std::fs::remove_file(&out_path)?; }
        entry.unpack(&out_path)?;
    }
    std::fs::remove_file(&marker)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn legacy_ini_enables_dll_spelled_extensions_and_ext_dir() {
        let ini = "; extension_dir = \"ext\"\n;extension=php_curl.dll\n;extension=php_xsl.dll\n;cgi.force_redirect = 1\n".to_string();
        let out = configured_ini(ini, true);
        assert!(out.contains("\nextension=php_curl.dll\n"), "{out}");
        assert!(out.contains(";extension=php_xsl.dll"), "only the chosen extensions are enabled");
        assert!(out.starts_with("extension_dir = \"ext\""));
        assert!(out.contains("cgi.force_redirect = 0"));
    }

    #[test]
    fn modern_ini_keeps_short_names() {
        let out = configured_ini(";extension_dir = \"ext\"\n;extension=curl\n".to_string(), false);
        assert_eq!(out, "extension_dir = \"ext\"\nextension=curl\n");
    }

    #[test]
    #[cfg(windows)]
    fn loader_failures_mean_a_missing_runtime() {
        assert!(is_missing_dll(0xC000_0135_u32 as i32));
        assert!(is_missing_dll(-1073741515)); // the same code as Windows reports it
        assert!(!is_missing_dll(1));
        assert!(!is_missing_dll(255));
    }

    fn write_zip(path: &Path, files: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        for (name, data) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn successful_extraction_is_complete_and_keeps_extra_files() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("8.2");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("php.ini"), "user edits").unwrap();
        let zip_path = tmp.path().join("php.zip");
        write_zip(&zip_path, &[("php.exe", b"bin"), ("ext/php_curl.dll", b"dll")]);

        extract_zip(&zip_path, &dest, "").unwrap();

        assert!(is_install_complete(&dest));
        assert_eq!(std::fs::read(dest.join("ext/php_curl.dll")).unwrap(), b"dll");
        assert_eq!(std::fs::read_to_string(dest.join("php.ini")).unwrap(), "user edits");
    }

    #[test]
    fn corrupt_archive_leaves_install_flagged_incomplete() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("8.2");
        std::fs::create_dir_all(&dest).unwrap();
        let zip_path = tmp.path().join("php.zip");
        std::fs::write(&zip_path, b"not a zip").unwrap();

        assert!(extract_zip(&zip_path, &dest, "").is_err());
        assert!(!is_install_complete(&dest));
    }

    #[cfg(not(windows))]
    fn write_tar_gz(path: &Path, files: &[(&str, &[u8], u32)], links: &[(&str, &str)]) {
        let gz = flate2::write::GzEncoder::new(std::fs::File::create(path).unwrap(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        for (name, data, mode) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(*mode);
            h.set_cksum();
            tar.append_data(&mut h, name, *data).unwrap();
        }
        for (name, target) in links {
            let mut h = tar::Header::new_gnu();
            h.set_entry_type(tar::EntryType::Symlink);
            h.set_size(0);
            tar.append_link(&mut h, name, target).unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
    }

    #[test]
    #[cfg(not(windows))]
    fn tar_gz_extraction_strips_the_prefix_and_keeps_modes_and_links() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("v22.0.0");
        std::fs::create_dir_all(&dest).unwrap();
        let archive = tmp.path().join("node.tar.gz");
        write_tar_gz(&archive,
            &[("node-v22.0.0-linux-x64/bin/node", b"elf", 0o755), ("node-v22.0.0-linux-x64/lib/npm-cli.js", b"js", 0o644)],
            &[("node-v22.0.0-linux-x64/bin/npm", "../lib/npm-cli.js")]);

        extract_tar_gz(&archive, &dest, "node-v22.0.0-linux-x64/").unwrap();

        assert!(is_install_complete(&dest));
        let mode = std::fs::metadata(dest.join("bin/node")).unwrap().permissions().mode();
        assert_eq!(mode & 0o111, 0o111, "executable bit kept");
        assert_eq!(std::fs::read_link(dest.join("bin/npm")).unwrap(), Path::new("../lib/npm-cli.js"));
        assert_eq!(std::fs::read(dest.join("bin/npm")).unwrap(), b"js");
    }

    #[test]
    #[cfg(not(windows))]
    fn tar_gz_extraction_replaces_existing_files() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("8.4");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("php-fpm"), b"old").unwrap();
        std::fs::write(dest.join("php.ini"), b"user edits").unwrap();
        let archive = tmp.path().join("fpm.tar.gz");
        write_tar_gz(&archive, &[("php-fpm", b"new", 0o755)], &[]);

        extract_tar_gz(&archive, &dest, "").unwrap();

        assert_eq!(std::fs::read(dest.join("php-fpm")).unwrap(), b"new");
        assert_eq!(std::fs::read(dest.join("php.ini")).unwrap(), b"user edits");
    }

    #[test]
    #[cfg(not(windows))]
    fn corrupt_tar_gz_leaves_install_flagged_incomplete() {
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("8.4");
        std::fs::create_dir_all(&dest).unwrap();
        let archive = tmp.path().join("bad.tar.gz");
        std::fs::write(&archive, b"not gzip").unwrap();
        assert!(extract_tar_gz(&archive, &dest, "").is_err());
        assert!(!is_install_complete(&dest));
    }

    async fn wait_for_php(state: &DownloadState, major: &str) -> DownloadProgress {
        let deadline = std::time::Instant::now() + Duration::from_secs(600);
        loop {
            let p = state.php.lock().get(major).cloned();
            if let Some(p) = p.filter(|p| !p.state.is_active()) { return p; }
            assert!(std::time::Instant::now() < deadline, "download did not finish in time");
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Real network: downloads the static PHP 8.4 php-fpm + CLI (~25 MB) from
    /// static-php-cli into a temp dir. Run with `cargo test -- --ignored`.
    #[tokio::test]
    #[cfg(not(windows))]
    #[ignore = "downloads ~25 MB from dl.static-php.dev"]
    async fn real_static_php_download_installs_into_temp_dir() {
        let tmp = TempDir::new().unwrap();
        let state = DownloadState::new();
        download_php("8.4", tmp.path(), state.clone());
        let done = wait_for_php(&state, "8.4").await;
        assert_eq!(done.state, TaskState::Done, "{}", done.message);
        let dest = tmp.path().join("8.4");
        assert!(dest.join("php-fpm").exists() && dest.join("php").exists());
        assert!(dest.join("php.ini").exists());
        assert!(is_install_complete(&dest));
        assert!(!dest.join("_download.tar.gz").exists());
    }

    /// Real network: downloads PHP 8.5 (~30 MB) from windows.php.net into a temp
    /// dir. Run with `cargo test -- --ignored`.
    #[tokio::test]
    #[cfg(windows)]
    #[ignore = "downloads ~30 MB from windows.php.net"]
    async fn real_php_download_installs_into_temp_dir() {
        let tmp = TempDir::new().unwrap();
        let state = DownloadState::new();
        download_php("8.5", tmp.path(), state.clone());
        let final_state = wait_for_php(&state, "8.5").await;

        assert_eq!(final_state.state, TaskState::Done, "{}", final_state.message);
        let dest = tmp.path().join("8.5");
        assert!(dest.join("php-cgi.exe").exists());
        assert!(dest.join("php.ini").exists(), "configure_ini must have run");
        assert!(is_install_complete(&dest));
        assert!(!dest.join("_download.zip").exists());
    }
}
