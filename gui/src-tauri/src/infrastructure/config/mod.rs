use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::infrastructure::{fs, state::AppState};

/// The live configuration, for HTTP handlers. Reads return a snapshot;
/// `replace` writes `config.json` first and publishes only after the write
/// succeeded, so memory and disk never disagree.
pub struct ConfigStore { state: Arc<AppState> }

impl ConfigStore {
    pub fn new(state: Arc<AppState>) -> Self { Self { state } }

    pub fn get(&self) -> Config { self.state.config.read().clone() }

    pub async fn replace(&self, new: Config) -> anyhow::Result<()> {
        let (to_save, base_dir) = (new.clone(), self.state.base_dir.clone());
        tokio::task::spawn_blocking(move || to_save.save(&base_dir)).await??;
        *self.state.config.write() = new;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub api_addr: String,
    pub base_dir: String,
    pub nginx_dir: String,
    pub php_dir: String,
    pub certs_dir: String,
    pub logs_dir: String,
    pub http_port: u16,
    pub https_port: u16,
    pub scanned_dirs: Vec<String>,
    pub default_php: String,
    pub custom_php_dirs: Vec<String>,
    pub os: String,
}

impl Config {
    /// A corrupt `config.json` is quarantined (see [`fs::load_json_or`]) rather
    /// than silently replaced, and the returned warning should be shown to the user.
    pub fn load(base_dir: &Path) -> fs::Loaded<Self> {
        fs::load_json_or(&base_dir.join("config.json"), || Self::default_with_base(base_dir))
    }

    pub fn save(&self, base_dir: &Path) -> anyhow::Result<()> {
        fs::atomic_write(&base_dir.join("config.json"), &serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    fn default_with_base(base_dir: &Path) -> Self {
        let base = base_dir.to_string_lossy().into_owned();
        Self {
            api_addr:        "127.0.0.1:7878".into(),
            nginx_dir:       format!("{}/nginx", base),
            php_dir:         format!("{}/php", base),
            certs_dir:       format!("{}/certs", base),
            logs_dir:        format!("{}/logs", base),
            base_dir:        base,
            http_port:       80,
            https_port:      443,
            scanned_dirs:    vec![],
            default_php:     "8.2".into(),
            custom_php_dirs: vec![],
            os:              std::env::consts::OS.into(),
        }
    }
}

pub fn resolve_base_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let portable = exe.parent().unwrap_or(&exe).join("data");
        if portable.join("config.json").exists() {
            return portable;
        }
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".phpenv")
}
