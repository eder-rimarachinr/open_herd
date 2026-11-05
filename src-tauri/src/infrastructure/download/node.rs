//! Node.js downloads from nodejs.org, verified against the release's
//! `SHASUMS256.txt`.

use std::{path::Path, sync::Arc};

use super::{fetch_zip, http_client, DownloadState};
use crate::domain::{node::NodeVersion, ports::{download::DownloadProgress, task::TaskState}};

const DIST: &str = "https://nodejs.org/dist";

/// Hash for `filename` in a `SHASUMS256.txt` body (`<hex>  <name>` lines).
pub(crate) fn sha_from_shasums(text: &str, filename: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        let name = parts.next()?;
        (name == filename && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}

fn set(progress: &DownloadState, version: &NodeVersion, state: TaskState, msg: &str, pct: u8) {
    progress.node.lock().insert(version.as_str().to_string(), DownloadProgress { state, message: msg.into(), percent: pct, error: None });
}

/// Starts the download as a task on the current Tokio runtime and returns at
/// once. `dest_dir` is the final `v<version>` folder; the zip's top-level
/// `node-v<version>-win-x64/` folder is stripped so `node.exe` lands in it
/// (the layout nvm-windows uses too).
// TODO(linux): fetch the `linux-x64` tarball instead.
pub fn download_node(version: &NodeVersion, dest_dir: &Path, progress: Arc<DownloadState>) {
    let version  = version.clone();
    let dest_dir = dest_dir.to_path_buf();
    tokio::spawn(async move {
        let v        = version.as_str();
        let filename = format!("node-v{v}-win-x64.zip");
        set(&progress, &version, TaskState::Downloading, &format!("Fetching checksums for Node {v}…"), 3);
        let sums_url = format!("{DIST}/v{v}/SHASUMS256.txt");
        let sums = match fetch_text(&sums_url).await {
            Ok(t) => t,
            Err(e) => { set(&progress, &version, TaskState::Error, &format!("Could not read {sums_url}: {e}"), 0); return; }
        };
        let Some(hash) = sha_from_shasums(&sums, &filename) else {
            set(&progress, &version, TaskState::Error, &format!("No SHA-256 for {filename} in SHASUMS256.txt — refusing to install an unverified build."), 0);
            return;
        };
        set(&progress, &version, TaskState::Downloading, &format!("Downloading Node {v}…"), 8);
        let url    = format!("{DIST}/v{v}/{filename}");
        let prefix = format!("node-v{v}-win-x64/");
        let ver2   = version.clone();
        let result = fetch_zip(&url, &dest_dir, &prefix, Some(&hash), &progress, move |p, pct, msg| {
            set(p, &ver2, TaskState::Downloading, msg, pct);
        }).await;
        match result {
            Ok(())  => set(&progress, &version, TaskState::Done, &format!("Node {v} installed"), 100),
            Err(e)  => set(&progress, &version, TaskState::Error, &e.to_string(), 0),
        }
    });
}

async fn fetch_text(url: &str) -> anyhow::Result<String> {
    let resp = http_client().get(url).send().await?;
    if !resp.status().is_success() { return Err(anyhow::anyhow!("HTTP {}", resp.status())); }
    Ok(resp.text().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUMS: &str = "\
1111111111111111111111111111111111111111111111111111111111111111  node-v22.19.0-win-x64.7z
abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd  node-v22.19.0-win-x64.zip
2222222222222222222222222222222222222222222222222222222222222222  node-v22.19.0-win-x86.zip
";

    #[test]
    fn sha_from_shasums_finds_exact_file() {
        assert_eq!(
            sha_from_shasums(SUMS, "node-v22.19.0-win-x64.zip").as_deref(),
            Some("abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd"),
        );
        assert_eq!(sha_from_shasums(SUMS, "node-v22.19.0-win-arm64.zip"), None);
        assert_eq!(sha_from_shasums(SUMS, "win-x64.zip"), None, "must match the whole name");
    }

    /// Real network: downloads a Node LTS zip (~30 MB) into a temp dir.
    /// Run with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore = "downloads ~30 MB from nodejs.org"]
    async fn node_download_real() {
        let tmp = tempfile::TempDir::new().unwrap();
        let state = DownloadState::new();
        let version = NodeVersion::parse("22.11.0").unwrap();
        let dest = tmp.path().join(version.dir_name());
        download_node(&version, &dest, state.clone());

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(600);
        let done = loop {
            let p = state.node.lock().get(version.as_str()).cloned();
            if let Some(p) = p.filter(|p| !p.state.is_active()) { break p; }
            assert!(std::time::Instant::now() < deadline, "download did not finish in time");
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        };
        assert_eq!(done.state, TaskState::Done, "{}", done.message);
        assert!(dest.join("node.exe").exists());
        assert!(super::super::is_install_complete(&dest));
        assert!(!dest.join("_download.zip").exists());
    }
}
