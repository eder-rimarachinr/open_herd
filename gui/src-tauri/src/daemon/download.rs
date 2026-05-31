use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use parking_lot::Mutex;

const NGINX_VERSION: &str = "1.26.3";
const NGINX_URL: &str = "https://nginx.org/download/nginx-1.26.3.zip";

#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadProgress {
    pub state: String, // pending | downloading | extracting | done | error
    pub message: String,
    pub percent: u8,
    pub error: Option<String>,
}

impl DownloadProgress {
    pub fn pending() -> Self {
        Self { state: "pending".into(), message: "Starting download…".into(), percent: 0, error: None }
    }
    pub fn done() -> Self {
        Self { state: "done".into(), message: "Nginx ready".into(), percent: 100, error: None }
    }
    pub fn error(e: &str) -> Self {
        Self { state: "error".into(), message: e.into(), percent: 0, error: Some(e.into()) }
    }
}

pub struct DownloadState {
    pub nginx: Mutex<Option<DownloadProgress>>,
}

impl DownloadState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { nginx: Mutex::new(None) })
    }
}

pub fn download_nginx(dest_dir: &Path, progress: Arc<DownloadState>) {
    let dest_dir = dest_dir.to_path_buf();
    std::thread::spawn(move || {
        *progress.nginx.lock() = Some(DownloadProgress {
            state: "downloading".into(),
            message: format!("Downloading nginx {}…", NGINX_VERSION),
            percent: 5,
            error: None,
        });

        match do_download(&dest_dir, &progress) {
            Ok(()) => { *progress.nginx.lock() = Some(DownloadProgress::done()); }
            Err(e) => { *progress.nginx.lock() = Some(DownloadProgress::error(&e.to_string())); }
        }
    });
}

fn do_download(dest_dir: &PathBuf, progress: &Arc<DownloadState>) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest_dir)?;

    let zip_path = dest_dir.join("nginx.zip");

    // Download
    {
        let mut resp = reqwest::blocking::get(NGINX_URL)?;
        let total = resp.content_length().unwrap_or(0);
        let mut file = std::fs::File::create(&zip_path)?;
        let mut downloaded = 0u64;
        let mut buf = [0u8; 8192];

        use std::io::Read;
        loop {
            let n = resp.read(&mut buf)?;
            if n == 0 { break; }
            file.write_all(&buf[..n])?;
            downloaded += n as u64;
            let pct = if total > 0 { (downloaded * 80 / total).min(80) as u8 } else { 40 };
            *progress.nginx.lock() = Some(DownloadProgress {
                state: "downloading".into(),
                message: format!("Downloading… {:.1} MB", downloaded as f64 / 1_048_576.0),
                percent: pct,
                error: None,
            });
        }
    }

    // Extract
    *progress.nginx.lock() = Some(DownloadProgress {
        state: "extracting".into(),
        message: "Extracting…".into(),
        percent: 85,
        error: None,
    });

    {
        let file = std::fs::File::open(&zip_path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let top_dir = format!("nginx-{}/", NGINX_VERSION);

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_string();

            // Strip top-level dir (nginx-1.26.3/) so files go directly into dest_dir
            let stripped = name.strip_prefix(&top_dir).unwrap_or(&name);
            if stripped.is_empty() { continue; }

            let out_path = dest_dir.join(stripped);
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
    }

    // Cleanup zip
    let _ = std::fs::remove_file(&zip_path);

    *progress.nginx.lock() = Some(DownloadProgress {
        state: "extracting".into(),
        message: "Configuring…".into(),
        percent: 95,
        error: None,
    });

    Ok(())
}
