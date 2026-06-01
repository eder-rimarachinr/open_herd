use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub path: String,
    pub php_version: String,
    pub project_type: String,
    pub ssl_enabled: bool,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub major: String,
    pub latest_patch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_patch: Option<String>,
    pub installed: bool,
    pub running: bool,
    pub has_update: bool,
    pub security_only: bool,
    pub end_of_life: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpVersion {
    pub version: String,
    pub major: String,
    pub binary_path: String,
    pub fpm_binary: String,
    pub fastcgi_addr: String,
    pub installed: bool,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NginxInfo {
    pub installed: bool,
    pub running: bool,
    pub version: String,
    pub binary_path: String,
    pub config_valid: Option<bool>,
    pub config_error: String,
    pub error_log: String,
    pub downloadable: bool,
    pub os: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NginxStatus {
    pub running: bool,
    pub version: Option<String>,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub status: String,
    pub version: String,
    pub uptime: String,
    pub os: String,
    pub php_versions: Vec<String>,
    pub nginx: NginxRunning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NginxRunning {
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub nginx: bool,
    pub php_versions: Vec<PhpVersionStatus>,
    pub all_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpVersionStatus {
    pub major: String,
    pub version: String,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallProgress {
    pub major: String,
    pub state: String,
    pub message: String,
    pub percent: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncTask {
    pub state: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpExtension {
    pub name: String,
    pub enabled: bool,
    pub category: String,  // "database" | "string" | "image" | "network" | "other"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpSetting {
    pub key: String,
    pub value: String,
    pub label: String,
    pub hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpIniConfig {
    pub major: String,
    pub ini_path: String,
    pub extensions: Vec<PhpExtension>,
    pub settings: Vec<PhpSetting>,
}
