use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use super::config::Config;
use super::models::{NginxStatus, PhpVersion, Site};

pub struct AppState {
    pub config: RwLock<Config>,
    pub base_dir: PathBuf,
    pub sites: RwLock<HashMap<String, Site>>,
    pub php_versions: RwLock<Vec<PhpVersion>>,
    pub nginx: RwLock<NginxStatus>,
    pub started_at: Instant,
}

impl AppState {
    pub fn new(base_dir: PathBuf, config: Config) -> Arc<Self> {
        let sites = load_sites(&base_dir);
        Arc::new(Self {
            config: RwLock::new(config),
            base_dir,
            sites: RwLock::new(sites),
            php_versions: RwLock::new(vec![]),
            nginx: RwLock::new(NginxStatus {
                running: false,
                version: None,
                pid: None,
            }),
            started_at: Instant::now(),
        })
    }
}

fn load_sites(base_dir: &PathBuf) -> HashMap<String, Site> {
    let path = base_dir.join("sites.json");
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(list) = serde_json::from_str::<Vec<Site>>(&data) {
            return list.into_iter().map(|s| (s.id.clone(), s)).collect();
        }
    }
    HashMap::new()
}

pub fn save_sites(state: &AppState) -> anyhow::Result<()> {
    let sites: Vec<Site> = state.sites.read().values().cloned().collect();
    let mut sorted = sites;
    sorted.sort_by(|a, b| a.domain.cmp(&b.domain));
    let path = state.base_dir.join("sites.json");
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&sorted)?)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}
