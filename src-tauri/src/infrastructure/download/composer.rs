//! `composer.phar` from getcomposer.org, verified against the release's
//! published `composer.phar.sha256sum`.

use std::{path::{Path, PathBuf}, sync::Arc};

use super::{http_client, node::sha_from_shasums, verify_sha256, DownloadState};
use crate::{
    domain::{
        composer::ComposerRelease,
        ports::{download::{DownloadProgress, PostInstall}, task::TaskState},
    },
    infrastructure::composer::{write_metadata, PHAR_NAME},
};

const DOWNLOADS: &str = "https://getcomposer.org/download";

fn set(progress: &DownloadState, state: TaskState, msg: &str, pct: u8) {
    *progress.composer.lock() = Some(DownloadProgress { state, message: msg.into(), percent: pct, error: None });
}

/// Starts the download as a task on the current Tokio runtime and returns at
/// once. The phar is downloaded beside the old one and renamed over it only
/// once its hash matches, so a failed update keeps the working Composer.
pub fn download_composer(release: &ComposerRelease, dest_dir: &Path, after: PostInstall, progress: Arc<DownloadState>) {
    let release  = release.clone();
    let dest_dir = dest_dir.to_path_buf();
    tokio::spawn(async move {
        match fetch(&release, &dest_dir, &progress).await {
            Ok(()) => {}
            Err(e) => { set(&progress, TaskState::Error, &e.to_string(), 0); return; }
        }
        set(&progress, TaskState::Configuring, "Creating the composer command…", 92);
        match after().await {
            Ok(())  => set(&progress, TaskState::Done, &format!("Composer {} installed", release.version), 100),
            Err(e)  => set(&progress, TaskState::Error, &format!("Composer {} was downloaded but the command could not be set up: {e}", release.version), 0),
        }
    });
}

async fn fetch(release: &ComposerRelease, dest_dir: &Path, progress: &Arc<DownloadState>) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;
    let v = &release.version;
    set(progress, TaskState::Downloading, &format!("Fetching the checksum of Composer {v}…"), 3);
    let sums_url = format!("{DOWNLOADS}/{v}/{PHAR_NAME}.sha256sum");
    let sums = fetch_text(&sums_url).await.map_err(|e| anyhow::anyhow!("Could not read {sums_url}: {e}"))?;
    let hash = sha_from_shasums(&sums, PHAR_NAME)
        .ok_or_else(|| anyhow::anyhow!("No SHA-256 in {sums_url} — refusing to install an unverified composer.phar."))?;

    set(progress, TaskState::Downloading, &format!("Downloading Composer {v}…"), 8);
    tokio::fs::create_dir_all(dest_dir).await?;
    let partial = dest_dir.join(format!("{PHAR_NAME}.download"));
    let url = format!("{DOWNLOADS}/{v}/{PHAR_NAME}");
    let result = async {
        let mut resp = http_client().get(&url).send().await?;
        if !resp.status().is_success() { return Err(anyhow::anyhow!("HTTP {} for {url}", resp.status())); }
        let total = resp.content_length().unwrap_or(0);
        let mut file = tokio::fs::File::create(&partial).await?;
        let mut downloaded = 0u64;
        while let Some(chunk) = resp.chunk().await? {
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            if let Some(ratio) = (downloaded * 80).checked_div(total) {
                let msg = format!("Downloading… {:.1} / {:.1} MB", downloaded as f64 / 1_048_576.0, total as f64 / 1_048_576.0);
                set(progress, TaskState::Downloading, &msg, (8 + ratio.min(80)) as u8);
            }
        }
        file.flush().await?;
        drop(file);
        set(progress, TaskState::Downloading, "Verifying integrity…", 90);
        let (partial, dest_dir, release) = (partial.clone(), dest_dir.to_path_buf(), release.clone());
        tokio::task::spawn_blocking(move || install_verified(&partial, &dest_dir, &hash, &release)).await?
    }.await;
    // Never leave the partial download behind, success or not.
    let _ = tokio::fs::remove_file(&partial).await;
    result
}

/// Checks the hash, moves the phar into place and records its version.
fn install_verified(partial: &Path, dest_dir: &Path, sha256: &str, release: &ComposerRelease) -> anyhow::Result<()> {
    verify_sha256(partial, sha256)?;
    let phar: PathBuf = dest_dir.join(PHAR_NAME);
    std::fs::rename(partial, &phar)?;
    write_metadata(dest_dir, release)?;
    Ok(())
}

async fn fetch_text(url: &str) -> anyhow::Result<String> {
    let resp = http_client().get(url).send().await?;
    if !resp.status().is_success() { return Err(anyhow::anyhow!("HTTP {}", resp.status())); }
    Ok(resp.text().await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::composer::read_metadata;

    #[test]
    fn verified_phar_replaces_the_old_one_and_records_its_version() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join(PHAR_NAME), b"old").unwrap();
        let partial = dir.join("composer.phar.download");
        std::fs::write(&partial, b"new").unwrap();
        // sha256("new")
        let hash = "11507a0e2f5e69d5dfa40a62a1bd7b6ee57e6bcd85c67c9b8431b36fff21c437";
        let release = ComposerRelease::new("2.11.2", 70205, false).unwrap();

        install_verified(&partial, dir, hash, &release).unwrap();

        assert_eq!(std::fs::read(dir.join(PHAR_NAME)).unwrap(), b"new");
        let meta = read_metadata(dir).unwrap();
        assert_eq!(meta.version.as_deref(), Some("2.11.2"));
        assert_eq!(meta.min_php, Some(70205));
    }

    #[test]
    fn bad_hash_keeps_the_old_phar() {
        let tmp = tempfile::TempDir::new().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join(PHAR_NAME), b"old").unwrap();
        let partial = dir.join("composer.phar.download");
        std::fs::write(&partial, b"tampered").unwrap();
        let release = ComposerRelease::new("2.11.2", 70205, false).unwrap();

        assert!(install_verified(&partial, dir, &"0".repeat(64), &release).is_err());
        assert_eq!(std::fs::read(dir.join(PHAR_NAME)).unwrap(), b"old");
        assert!(read_metadata(dir).is_none());
    }

    /// Real network: downloads the newest composer.phar (~4 MB) into a temp
    /// dir. Run with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "downloads ~4 MB from getcomposer.org"]
    async fn composer_download_real() {
        use crate::domain::ports::composer::ComposerReleasesPort;
        let tmp = tempfile::TempDir::new().unwrap();
        let releases = crate::infrastructure::composer::releases::HttpComposerReleases::new().releases().await.unwrap();
        let newest = crate::domain::composer::select_release(&releases, 80500).unwrap().clone();
        let state = DownloadState::new();
        download_composer(&newest, tmp.path(), Box::new(|| Box::pin(async { Ok(()) })), state.clone());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
        let done = loop {
            let p = state.composer.lock().clone();
            if let Some(p) = p.filter(|p| !p.state.is_active()) { break p; }
            assert!(std::time::Instant::now() < deadline, "download did not finish in time");
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        };
        assert_eq!(done.state, TaskState::Done, "{}", done.message);
        assert!(tmp.path().join(PHAR_NAME).metadata().unwrap().len() > 1_000_000);
        assert!(!tmp.path().join("composer.phar.download").exists());
    }
}
