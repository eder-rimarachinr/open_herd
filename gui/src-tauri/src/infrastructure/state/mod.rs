use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::infrastructure::{
    config::Config,
    dto::Site,
    nginx::process::NginxProcess,
    php::process::PhpProcesses,
};

pub struct AppState {
    pub config:      RwLock<Config>,
    pub base_dir:    PathBuf,
    pub sites:       RwLock<HashMap<String, Site>>,
    pub nginx_proc:  Arc<NginxProcess>,
    pub php_proc:    Arc<PhpProcesses>,
    pub started_at:  Instant,
    write_lock:      parking_lot::Mutex<()>,
}

impl AppState {
    pub fn new(base_dir: PathBuf, config: Config) -> Arc<Self> {
        let sites = load_sites(&base_dir);
        Arc::new(Self {
            config:       RwLock::new(config),
            base_dir,
            sites:        RwLock::new(sites),
            nginx_proc:   NginxProcess::new(),
            php_proc:     PhpProcesses::new(),
            started_at:   Instant::now(),
            write_lock:   parking_lot::Mutex::new(()),
        })
    }

    pub fn atomic_write(&self, path: &std::path::Path, data: &[u8]) -> anyhow::Result<()> {
        let _guard = self.write_lock.lock();
        let tmp    = path.with_extension("json.tmp");
        let result = (|| -> anyhow::Result<()> { std::fs::write(&tmp, data)?; std::fs::rename(&tmp, path)?; Ok(()) })();
        if result.is_err() { let _ = std::fs::remove_file(&tmp); }
        result
    }
}

pub fn load_sites(base_dir: &PathBuf) -> HashMap<String, Site> {
    let path = base_dir.join("sites.json");
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(list) = serde_json::from_str::<Vec<Site>>(&data) {
            return list.into_iter().map(|s| (s.id.clone(), s)).collect();
        }
    }
    HashMap::new()
}

pub fn save_sites(state: &AppState) -> anyhow::Result<()> {
    let sites = state.sites.read();
    let mut sorted: Vec<Site> = sites.values().cloned().collect();
    sorted.sort_by(|a, b| a.domain.cmp(&b.domain));
    let path = state.base_dir.join("sites.json");
    let data = serde_json::to_vec_pretty(&sorted)?;
    state.atomic_write(&path, &data)
}
