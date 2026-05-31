use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Site {
    pub id: String,
    pub domain: String,
    pub path: String,
    #[serde(rename = "projectType")]
    pub project_type: String,
    #[serde(rename = "phpVersion")]
    pub php_version: String,
    pub ssl: bool,
    pub nginx: bool,
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhpVersion {
    pub version: String,
    pub path: String,
    pub running: bool,
    #[serde(rename = "isDefault")]
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    Running,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NginxStatus {
    pub running: bool,
    pub version: Option<String>,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesStatus {
    pub nginx: NginxStatus,
    pub php: Vec<PhpVersion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub ok: bool,
    pub version: String,
    pub uptime: u64,
    pub services: ServicesStatus,
}
