/// DTOs de serialización JSON — estructuras legacy para compatibilidad con la GUI.
/// No son entidades de dominio; son representaciones de respuesta HTTP.
use serde::{Deserialize, Serialize};

use crate::domain::ports::task::TaskState;

/// The domain enum stays serde-free; on the wire it is its lowercase name.
impl Serialize for TaskState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct Site {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub path: String,
    pub php_version: String,
    #[cfg_attr(test, ts(as = "ts_bindings::ProjectKind"))]
    pub project_type: String,
    pub ssl_enabled: bool,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct CatalogEntry {
    pub major: String,
    pub latest_patch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub installed_patch: Option<String>,
    pub installed: bool,
    pub running: bool,
    pub has_update: bool,
    pub security_only: bool,
    pub end_of_life: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export, rename = "PHPVersion"))]
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
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct NginxStatus {
    pub running: bool,
    pub version: Option<String>,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct DaemonStatus {
    pub status: String,
    pub version: String,
    pub uptime: String,
    pub os: String,
    pub php_versions: Vec<String>,
    pub nginx: NginxRunning,
    /// Problems detected while loading persisted data at startup.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct NginxRunning { pub running: bool }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct ServiceStatus {
    pub nginx: bool,
    pub php_versions: Vec<PhpVersionStatus>,
    pub all_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct PhpVersionStatus {
    pub major: String,
    pub version: String,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct InstallProgress {
    pub major: String,
    #[cfg_attr(test, ts(as = "ts_bindings::TaskStateWire"))]
    pub state: TaskState,
    pub message: String,
    pub percent: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct AsyncTask {
    #[cfg_attr(test, ts(as = "ts_bindings::TaskStateWire"))]
    pub state: TaskState,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(test, ts(optional))]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct PhpExtension {
    pub name: String,
    pub enabled: bool,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct PhpSetting {
    pub key: String,
    pub value: String,
    pub label: String,
    pub hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(test, derive(ts_rs::TS), ts(export))]
pub struct PhpIniConfig {
    pub major: String,
    pub ini_path: String,
    pub extensions: Vec<PhpExtension>,
    pub settings: Vec<PhpSetting>,
}

// ── php.ini view → HTTP DTOs ──────────────────────────────────────────────────

use crate::application::php::{get_php_ini::PhpIniView, ini_parser::{ExtensionState, SettingValue}};

impl From<ExtensionState> for PhpExtension {
    fn from(e: ExtensionState) -> Self { Self { name: e.name, enabled: e.enabled, category: e.category } }
}

impl From<SettingValue> for PhpSetting {
    fn from(s: SettingValue) -> Self { Self { key: s.key, value: s.value, label: s.label, hint: s.hint } }
}

impl From<PhpIniView> for PhpIniConfig {
    fn from(v: PhpIniView) -> Self {
        Self {
            major: v.major,
            ini_path: v.path,
            extensions: v.extensions.into_iter().map(PhpExtension::from).collect(),
            settings: v.settings.into_iter().map(PhpSetting::from).collect(),
        }
    }
}

/// TypeScript-only mirrors of values that are sent as plain strings, so the
/// generated bindings carry the exact union of allowed values. Test-only: the
/// exhaustive `From` impls stop compiling if the domain gains a variant, and
/// the tests check every name matches what the API really sends.
#[cfg(test)]
pub mod ts_bindings {
    use serde::Serialize;
    use crate::domain::{ports::task::TaskState, site::entity::ProjectType};

    #[derive(Serialize, ts_rs::TS)]
    #[serde(rename_all = "lowercase")]
    #[ts(export, rename = "TaskState")]
    pub enum TaskStateWire { Idle, Pending, Running, Downloading, Extracting, Configuring, Done, Error }

    impl From<TaskState> for TaskStateWire {
        fn from(s: TaskState) -> Self {
            match s {
                TaskState::Idle => Self::Idle,
                TaskState::Pending => Self::Pending,
                TaskState::Running => Self::Running,
                TaskState::Downloading => Self::Downloading,
                TaskState::Extracting => Self::Extracting,
                TaskState::Configuring => Self::Configuring,
                TaskState::Done => Self::Done,
                TaskState::Error => Self::Error,
            }
        }
    }

    #[derive(Serialize, ts_rs::TS)]
    #[serde(rename_all = "lowercase")]
    #[ts(export, rename = "ProjectType")]
    pub enum ProjectKind { Laravel, CodeIgniter4, CodeIgniter3, WordPress, Spa, Static, Generic }

    impl From<&ProjectType> for ProjectKind {
        fn from(p: &ProjectType) -> Self {
            match p {
                ProjectType::Laravel => Self::Laravel,
                ProjectType::CodeIgniter4 => Self::CodeIgniter4,
                ProjectType::CodeIgniter3 => Self::CodeIgniter3,
                ProjectType::WordPress => Self::WordPress,
                ProjectType::Spa => Self::Spa,
                ProjectType::Static => Self::Static,
                ProjectType::Generic => Self::Generic,
            }
        }
    }

    #[test]
    fn task_state_names_match_the_wire() {
        use TaskState::*;
        for s in [Idle, Pending, Running, Downloading, Extracting, Configuring, Done, Error] {
            assert_eq!(
                serde_json::to_string(&TaskStateWire::from(s)).unwrap(),
                serde_json::to_string(&s).unwrap(),
            );
        }
    }

    #[test]
    fn project_type_names_match_the_wire() {
        use ProjectType::*;
        for p in [Laravel, CodeIgniter4, CodeIgniter3, WordPress, Spa, Static, Generic] {
            assert_eq!(serde_json::to_string(&ProjectKind::from(&p)).unwrap(), format!("\"{p}\""));
        }
    }
}
