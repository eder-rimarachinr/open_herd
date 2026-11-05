#![cfg(test)]

use async_trait::async_trait;
use chrono::NaiveDate;
use parking_lot::Mutex;
use std::{path::{Path, PathBuf}, sync::Arc};

use super::{
    activate_node::ActivateNodeUseCase, install_node::InstallNodeUseCase,
    list_node_releases::ListNodeReleasesUseCase, uninstall_node::UninstallNodeUseCase,
};
use crate::domain::{
    errors::{ApplicationError, DomainError, InfrastructureError},
    node::{NodeReleaseInfo, NodeVersion},
    ports::{
        download::{DownloadProgress, DownloadProgressPort},
        node::{NodeEnvironment, NodeInstall, NodeManagerMode, NodeManagerPort, NodeReleasesPort},
        task::TaskState,
    },
};

// ── Doubles ───────────────────────────────────────────────────────────────────

fn v(s: &str) -> NodeVersion { NodeVersion::parse(s).unwrap() }

struct FakeManager { installed: Mutex<Vec<NodeVersion>>, active: Mutex<Option<NodeVersion>> }

impl FakeManager {
    fn new(installed: &[&str], active: Option<&str>) -> Arc<Self> {
        Arc::new(Self { installed: Mutex::new(installed.iter().map(|s| v(s)).collect()), active: Mutex::new(active.map(v)) })
    }
}

#[async_trait]
impl NodeManagerPort for FakeManager {
    async fn environment(&self) -> Result<NodeEnvironment, InfrastructureError> {
        Ok(NodeEnvironment {
            mode: NodeManagerMode::NvmWindows,
            store_dir: PathBuf::from("/nvm"),
            link_path: PathBuf::from("/nvm4w/nodejs"),
            installed: self.installed.lock().iter().map(|v| NodeInstall { version: v.clone(), path: PathBuf::from("/nvm").join(v.dir_name()) }).collect(),
            active: self.active.lock().clone(),
            warnings: vec![],
        })
    }
    async fn activate(&self, version: &NodeVersion) -> Result<(), InfrastructureError> {
        *self.active.lock() = Some(version.clone());
        Ok(())
    }
    async fn uninstall(&self, version: &NodeVersion) -> Result<(), InfrastructureError> {
        self.installed.lock().retain(|i| i != version);
        Ok(())
    }
}

#[derive(Default)]
struct FakeDownloads { node: Mutex<Option<(String, TaskState)>>, started: Mutex<Vec<(String, PathBuf)>> }

impl FakeDownloads {
    fn in_flight(version: &str) -> Arc<Self> {
        Arc::new(Self { node: Mutex::new(Some((version.into(), TaskState::Downloading))), ..Default::default() })
    }
}

#[async_trait]
impl DownloadProgressPort for FakeDownloads {
    async fn start_nginx_download(&self, _: &Path) {}
    async fn nginx_progress(&self) -> Option<DownloadProgress> { None }
    async fn start_php_download(&self, _: &str, _: &Path) {}
    async fn php_progress(&self, _: &str) -> Option<DownloadProgress> { None }
    async fn start_node_download(&self, version: &NodeVersion, dest_dir: &Path) {
        self.started.lock().push((version.as_str().into(), dest_dir.to_path_buf()));
    }
    async fn node_progress(&self, version: &NodeVersion) -> Option<DownloadProgress> {
        self.node.lock().as_ref().filter(|(ver, _)| ver == version.as_str())
            .map(|(_, state)| DownloadProgress { state: *state, message: String::new(), percent: 10, error: None })
    }
}

struct FakeReleases(Vec<NodeReleaseInfo>);

#[async_trait]
impl NodeReleasesPort for FakeReleases {
    async fn releases(&self, _refresh: bool) -> Result<Vec<NodeReleaseInfo>, InfrastructureError> { Ok(self.0.clone()) }
}

fn rel(ver: &str, date: &str, lts: Option<&str>) -> NodeReleaseInfo {
    NodeReleaseInfo {
        version: v(ver), date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
        lts: lts.map(String::from), security: false, has_win_x64_zip: true,
    }
}

fn domain_err(e: ApplicationError) -> DomainError {
    match e { ApplicationError::Domain(d) => d, other => panic!("expected a domain error, got {other}") }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn activate_requires_installed() {
    let mgr = FakeManager::new(&["22.19.0"], None);
    let uc = ActivateNodeUseCase::new(mgr.clone());
    assert_eq!(domain_err(uc.execute("24.21.0").await.unwrap_err()), DomainError::NodeVersionNotInstalled("24.21.0".into()));
    let env = uc.execute("v22.19.0").await.unwrap();
    assert_eq!(env.active, Some(v("22.19.0")));
}

#[tokio::test]
async fn activate_rejects_malformed_version() {
    let uc = ActivateNodeUseCase::new(FakeManager::new(&[], None));
    assert!(matches!(domain_err(uc.execute("..\\x").await.unwrap_err()), DomainError::InvalidNodeVersion(_)));
}

#[tokio::test]
async fn uninstall_rejects_active() {
    let mgr = FakeManager::new(&["22.19.0"], Some("22.19.0"));
    let uc = UninstallNodeUseCase::new(mgr.clone(), Arc::new(FakeDownloads::default()));
    assert_eq!(domain_err(uc.execute("22.19.0").await.unwrap_err()), DomainError::NodeVersionActive("22.19.0".into()));
    assert_eq!(mgr.installed.lock().len(), 1);
}

#[tokio::test]
async fn uninstall_rejects_in_flight_download() {
    let mgr = FakeManager::new(&["22.19.0"], None);
    let uc = UninstallNodeUseCase::new(mgr.clone(), FakeDownloads::in_flight("22.19.0"));
    assert_eq!(domain_err(uc.execute("22.19.0").await.unwrap_err()), DomainError::NodeVersionBusy("22.19.0".into()));
}

#[tokio::test]
async fn uninstall_removes_installed_and_rejects_unknown() {
    let mgr = FakeManager::new(&["22.19.0", "24.21.0"], Some("24.21.0"));
    let uc = UninstallNodeUseCase::new(mgr.clone(), Arc::new(FakeDownloads::default()));
    uc.execute("22.19.0").await.unwrap();
    assert_eq!(*mgr.installed.lock(), vec![v("24.21.0")]);
    assert_eq!(domain_err(uc.execute("22.19.0").await.unwrap_err()), DomainError::NodeVersionNotInstalled("22.19.0".into()));
}

#[tokio::test]
async fn install_skips_when_installed() {
    let downloads = Arc::new(FakeDownloads::default());
    let uc = InstallNodeUseCase::new(FakeManager::new(&["22.19.0"], None), downloads.clone());
    assert_eq!(uc.execute("22.19.0").await.unwrap(), TaskState::Done);
    assert!(downloads.started.lock().is_empty());
}

#[tokio::test]
async fn install_starts_download_into_store_and_reuses_in_flight() {
    let downloads = Arc::new(FakeDownloads::default());
    let uc = InstallNodeUseCase::new(FakeManager::new(&[], None), downloads.clone());
    assert_eq!(uc.execute("24.21.0").await.unwrap(), TaskState::Pending);
    assert_eq!(*downloads.started.lock(), vec![("24.21.0".to_string(), PathBuf::from("/nvm").join("v24.21.0"))]);

    let busy = FakeDownloads::in_flight("24.21.0");
    let uc = InstallNodeUseCase::new(FakeManager::new(&[], None), busy.clone());
    assert_eq!(uc.execute("24.21.0").await.unwrap(), TaskState::Downloading);
    assert!(busy.started.lock().is_empty());
}

#[tokio::test]
async fn releases_mark_installed_active_and_updates() {
    let releases = Arc::new(FakeReleases(vec![
        rel("24.21.0", "2026-10-01", Some("Krypton")),
        rel("22.21.0", "2026-08-01", Some("Jod")),
    ]));
    let mgr = FakeManager::new(&["24.21.0", "22.19.0"], Some("24.21.0"));
    let uc = ListNodeReleasesUseCase::new(mgr, releases);
    let today = NaiveDate::parse_from_str("2026-11-05", "%Y-%m-%d").unwrap();
    let views = uc.execute_on(false, today).await.unwrap();
    assert_eq!(views.len(), 2);
    assert!(views[0].installed && views[0].active && !views[0].update_available);
    assert_eq!(views[1].release.version.as_str(), "22.21.0");
    assert!(!views[1].installed && !views[1].active);
    assert!(views[1].update_available, "22.19.0 is installed and 22.21.0 is newer");
}
