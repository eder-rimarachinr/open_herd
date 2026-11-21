//! The release list from `https://getcomposer.org/versions`, cached for an hour.

use async_trait::async_trait;
use parking_lot::Mutex;
use serde::Deserialize;
use std::time::{Duration, Instant};

use crate::{
    domain::{composer::ComposerRelease, errors::InfrastructureError, ports::composer::ComposerReleasesPort},
    infrastructure::download::http_client,
};

const VERSIONS_URL: &str = "https://getcomposer.org/versions";
const CACHE_TTL: Duration = Duration::from_secs(3600);

#[derive(Deserialize)]
struct RawRelease {
    version: String,
    #[serde(rename = "min-php", default)]
    min_php: u32,
    #[serde(default)]
    lts: bool,
}

#[derive(Deserialize)]
struct RawVersions {
    #[serde(default)]
    stable: Vec<RawRelease>,
}

/// The `stable` channel of `versions` (the newest release and the 2.2 LTS),
/// skipping entries whose version is not a plain release number.
pub fn parse_versions(json: &str) -> Result<Vec<ComposerRelease>, InfrastructureError> {
    let raw: RawVersions = serde_json::from_str(json)
        .map_err(|e| InfrastructureError::Serialization(format!("getcomposer.org versions: {e}")))?;
    Ok(raw.stable.into_iter()
        .filter_map(|r| ComposerRelease::new(&r.version, r.min_php, r.lts).ok())
        .collect())
}

#[derive(Default)]
pub struct HttpComposerReleases {
    cache: Mutex<Option<(Instant, Vec<ComposerRelease>)>>,
}

impl HttpComposerReleases {
    pub fn new() -> Self { Self::default() }

    fn cached(&self) -> Option<Vec<ComposerRelease>> {
        self.cache.lock().as_ref().filter(|(at, _)| at.elapsed() < CACHE_TTL).map(|(_, list)| list.clone())
    }
}

#[async_trait]
impl ComposerReleasesPort for HttpComposerReleases {
    async fn releases(&self) -> Result<Vec<ComposerRelease>, InfrastructureError> {
        if let Some(list) = self.cached() { return Ok(list); }
        let failed = |e: reqwest::Error| InfrastructureError::DownloadFailed(format!("{VERSIONS_URL}: {e}"));
        let resp = http_client().get(VERSIONS_URL).send().await.map_err(failed)?;
        if !resp.status().is_success() {
            return Err(InfrastructureError::DownloadFailed(format!("{VERSIONS_URL}: HTTP {}", resp.status())));
        }
        let list = parse_versions(&resp.text().await.map_err(failed)?)?;
        *self.cache.lock() = Some((Instant::now(), list.clone()));
        Ok(list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_stable_channel() {
        let json = r#"{
            "stable": [
                {"path": "/download/2.11.2/composer.phar", "version": "2.11.2", "min-php": 70205, "lts": false},
                {"path": "/download/2.2.32/composer.phar", "version": "2.2.32", "min-php": 50300, "lts": true}
            ],
            "snapshot": [{"path": "/composer.phar", "version": "8a53515e753d9f60f1ed338cd40e0fae9464af52", "min-php": 70205}]
        }"#;
        let list = parse_versions(json).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], ComposerRelease::new("2.11.2", 70205, false).unwrap());
        assert!(list[1].lts);
    }

    #[test]
    fn skips_unsafe_versions() {
        let json = r#"{"stable": [{"version": "../../x", "min-php": 1}, {"version": "2.9.0", "min-php": 70205}]}"#;
        assert_eq!(parse_versions(json).unwrap().len(), 1);
        assert!(parse_versions("not json").is_err());
    }
}
