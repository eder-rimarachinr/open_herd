use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::infrastructure::{
    config::Config,
    download::DownloadState,
    dto::{NginxStatus, Site},
    nginx::process::NginxProcess,
    php::process::PhpProcesses,
    ssl::mkcert::SslTasks,
};

pub struct AppState {
    pub config:      RwLock<Config>,
    pub base_dir:    PathBuf,
    pub sites:       RwLock<HashMap<String, Site>>,
    pub nginx:       RwLock<NginxStatus>,
    pub nginx_proc:  Arc<NginxProcess>,
    pub php_proc:    Arc<PhpProcesses>,
    pub downloads:   Arc<DownloadState>,
    pub ssl_tasks:   SslTasks,
    pub started_at:  Instant,
    pub daemon_log:  RwLock<Vec<String>>,
    write_lock:      parking_lot::Mutex<()>,
}

impl AppState {
    pub fn new(base_dir: PathBuf, config: Config) -> Arc<Self> {
        let sites = load_sites(&base_dir);
        Arc::new(Self {
            config:       RwLock::new(config),
            base_dir,
            sites:        RwLock::new(sites),
            nginx:        RwLock::new(NginxStatus { running: false, version: None, pid: None }),
            nginx_proc:   NginxProcess::new(),
            php_proc:     PhpProcesses::new(),
            downloads:    DownloadState::new(),
            ssl_tasks:    crate::infrastructure::ssl::mkcert::new_ssl_tasks(),
            started_at:   Instant::now(),
            daemon_log:   RwLock::new(vec![]),
            write_lock:   parking_lot::Mutex::new(()),
        })
    }

    pub fn log(&self, msg: String) {
        let mut log   = self.daemon_log.write();
        let entry     = format!("[{}] {}", chrono::Local::now().format("%H:%M:%S"), msg);
        eprintln!("{}", entry);
        log.push(entry);
        if log.len() > 200 { let excess = log.len() - 200; log.drain(0..excess); }
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
