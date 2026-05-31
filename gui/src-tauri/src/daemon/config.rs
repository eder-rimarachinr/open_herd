use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(rename = "apiAddr")]
    pub api_addr: String,
    #[serde(rename = "baseDir")]
    pub base_dir: String,
    #[serde(rename = "defaultPHP")]
    pub default_php: String,
    #[serde(rename = "scanDirs")]
    pub scan_dirs: Vec<String>,
    #[serde(rename = "nginxPort")]
    pub nginx_port: u16,
    #[serde(rename = "nginxSSLPort")]
    pub nginx_ssl_port: u16,
}

impl Config {
    pub fn load(base_dir: &PathBuf) -> Self {
        let path = base_dir.join("config.json");
        if let Ok(data) = std::fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str(&data) {
                return cfg;
            }
        }
        Self::default_with_base(base_dir)
    }

    pub fn save(&self, base_dir: &PathBuf) -> anyhow::Result<()> {
        let path = base_dir.join("config.json");
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    fn default_with_base(base_dir: &PathBuf) -> Self {
        Self {
            api_addr: "127.0.0.1:7878".into(),
            base_dir: base_dir.to_string_lossy().into(),
            default_php: "8.2".into(),
            scan_dirs: vec![],
            nginx_port: 80,
            nginx_ssl_port: 443,
        }
    }
}

pub fn resolve_base_dir() -> PathBuf {
    // Portable mode: data/config.json next to exe
    if let Ok(exe) = std::env::current_exe() {
        let portable = exe.parent().unwrap_or(&exe).join("data");
        if portable.join("config.json").exists() {
            return portable;
        }
    }
    // Standard mode: ~/.phpenv
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".phpenv")
}
