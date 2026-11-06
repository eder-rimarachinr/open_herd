//! The release list from `https://nodejs.org/dist/index.json`, cached for an hour.

use async_trait::async_trait;
use chrono::NaiveDate;
use parking_lot::Mutex;
use serde::Deserialize;
use std::time::{Duration, Instant};

use crate::{
    domain::{errors::InfrastructureError, node::{NodeReleaseInfo, NodeVersion}, ports::node::NodeReleasesPort},
    infrastructure::download::http_client,
};

const INDEX_URL: &str = "https://nodejs.org/dist/index.json";
const CACHE_TTL: Duration = Duration::from_secs(3600);

/// The `files` entry of the build Open Herd downloads on this machine.
const BUILD_FILE: &str = if cfg!(windows) {
    "win-x64-zip"
} else if cfg!(target_arch = "aarch64") {
    "linux-arm64"
} else {
    "linux-x64"
};

#[derive(Deserialize)]
struct RawRelease {
    version: String,
    date: String,
    #[serde(default)]
    files: Vec<String>,
    /// `false` or the LTS codename.
    #[serde(default)]
    lts: serde_json::Value,
    #[serde(default)]
    security: bool,
}

/// Parses `index.json`, skipping entries Open Herd can't use (prereleases,
/// malformed versions or dates).
pub fn parse_index(json: &str) -> Result<Vec<NodeReleaseInfo>, InfrastructureError> {
    let raw: Vec<RawRelease> = serde_json::from_str(json)
        .map_err(|e| InfrastructureError::Serialization(format!("nodejs.org index.json: {e}")))?;
    Ok(raw.into_iter().filter_map(|r| {
        Some(NodeReleaseInfo {
            version: NodeVersion::parse(&r.version).ok()?,
            date: NaiveDate::parse_from_str(&r.date, "%Y-%m-%d").ok()?,
            lts: r.lts.as_str().map(str::to_string),
            security: r.security,
            has_build: r.files.iter().any(|f| f == BUILD_FILE),
        })
    }).collect())
}

#[derive(Default)]
pub struct HttpNodeReleases {
    cache: Mutex<Option<(Instant, Vec<NodeReleaseInfo>)>>,
}

impl HttpNodeReleases {
    pub fn new() -> Self { Self::default() }

    fn cached(&self) -> Option<Vec<NodeReleaseInfo>> {
        self.cache.lock().as_ref()
            .filter(|(at, _)| at.elapsed() < CACHE_TTL)
            .map(|(_, list)| list.clone())
    }
}

#[async_trait]
impl NodeReleasesPort for HttpNodeReleases {
    async fn releases(&self, refresh: bool) -> Result<Vec<NodeReleaseInfo>, InfrastructureError> {
        if !refresh {
            if let Some(list) = self.cached() { return Ok(list); }
        }
        let fail = |e: String| InfrastructureError::DownloadFailed(format!("No se pudo leer la lista de versiones de nodejs.org: {e}"));
        let resp = http_client().get(INDEX_URL).send().await.map_err(|e| fail(e.to_string()))?;
        if !resp.status().is_success() { return Err(fail(format!("HTTP {}", resp.status()))); }
        let body = resp.text().await.map_err(|e| fail(e.to_string()))?;
        let list = parse_index(&body)?;
        *self.cache.lock() = Some((Instant::now(), list.clone()));
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_index_json_fixture() {
        let json = r#"[
          {"version":"v27.0.0-alpha.0.0.0","date":"2026-10-29","files":["win-x64-zip"],"lts":false,"security":false},
          {"version":"v26.13.0","date":"2026-10-28","files":["linux-x64","win-x64-zip"],"lts":"Lithium","security":false},
          {"version":"v26.12.1","date":"2026-10-21","files":["win-x64-7z"],"lts":false,"security":true}
        ]"#;
        let list = parse_index(json).unwrap();
        assert_eq!(list.len(), 2, "the prerelease is skipped");
        assert_eq!(list[0].version.as_str(), "26.13.0");
        assert_eq!(list[0].lts.as_deref(), Some("Lithium"));
        assert!(list[0].has_build);
        assert_eq!(list[1].lts, None);
        assert!(list[1].security);
        assert!(!list[1].has_build);
    }

    #[test]
    fn parse_index_rejects_non_json() {
        assert!(matches!(parse_index("<html>"), Err(InfrastructureError::Serialization(_))));
    }
}
