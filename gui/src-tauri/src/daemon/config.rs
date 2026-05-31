use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
        let base = base_dir.to_string_lossy().into_owned();
        Self {
            api_addr: "127.0.0.1:7878".into(),
            nginx_dir: format!("{}/nginx", base),
            php_dir: format!("{}/php", base),
            certs_dir: format!("{}/certs", base),
            logs_dir: format!("{}/logs", base),
            base_dir: base,
            http_port: 80,
            https_port: 443,
            scanned_dirs: vec![],
            default_php: "8.2".into(),
            custom_php_dirs: vec![],
            os: std::env::consts::OS.into(),
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
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".phpenv")
}
