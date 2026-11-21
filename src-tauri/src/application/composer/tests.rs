#![cfg(test)]

use async_trait::async_trait;
use parking_lot::Mutex;
use std::{path::{Path, PathBuf}, sync::Arc};

use super::{
    get_composer_status::GetComposerStatusUseCase, install_composer::InstallComposerUseCase,
    sync_composer_command::SyncComposerCommandUseCase,
};
use crate::domain::{
    composer::ComposerRelease,
    errors::{ApplicationError, DomainError, InfrastructureError},
    node::NodeVersion,
    ports::{
        composer::{ComposerEnvironment, ComposerInstall, ComposerManagerPort, ComposerReleasesPort},
        download::{DownloadProgress, DownloadProgressPort, PostInstall},
        php_ini::{PhpIniFile, PhpIniStore},
        process_manager::{PhpDetectorPort, PhpInstallation},
        task::TaskState,
    },
};

// ── Doubles ───────────────────────────────────────────────────────────────────

#[derive(Default)]
struct FakeManager {
    installed: Mutex<Option<ComposerInstall>>,
    command_php: Mutex<Option<PathBuf>>,
    modules: Vec<String>,
}

impl FakeManager {
    fn with(version: Option<&str>, min_php: u32, command_php: Option<&str>, modules: &[&str]) -> Arc<Self> {
        Arc::new(Self {
            installed: Mutex::new(version.map(|v| ComposerInstall { version: Some(v.into()), min_php: Some(min_php) })),
            command_php: Mutex::new(command_php.map(PathBuf::from)),
            modules: modules.iter().map(|m| (*m).to_string()).collect(),
        })
    }
}

#[async_trait]
impl ComposerManagerPort for FakeManager {
    async fn environment(&self) -> Result<ComposerEnvironment, InfrastructureError> {
        Ok(ComposerEnvironment {
            phar_path: PathBuf::from("/base/composer/composer.phar"),
            command_path: PathBuf::from("/base/bin/composer"),
            bin_dir: PathBuf::from("/base/bin"),
            installed: self.installed.lock().clone(),
            command_php: self.command_php.lock().clone(),
            warnings: vec![],
        })
    }
    async fn write_command(&self, php: &Path) -> Result<(), InfrastructureError> {
        *self.command_php.lock() = Some(php.to_path_buf());
        Ok(())
    }
    async fn php_modules(&self, _: &Path) -> Result<Vec<String>, InfrastructureError> { Ok(self.modules.clone()) }
}

struct FakeReleases { offline: bool }

#[async_trait]
impl ComposerReleasesPort for FakeReleases {
    async fn releases(&self) -> Result<Vec<ComposerRelease>, InfrastructureError> {
        if self.offline { return Err(InfrastructureError::DownloadFailed("offline".into())); }
        Ok(vec![ComposerRelease::new("2.11.2", 70205, false).unwrap(), ComposerRelease::new("2.2.32", 50300, true).unwrap()])
    }
}

struct FakeDetector(Vec<PhpInstallation>);

fn php(major: &str, version: &str) -> PhpInstallation {
    PhpInstallation { major: major.into(), version: version.into(), binary_path: abs(&format!("php/{major}/php")) }
}

/// An absolute path on this platform.
fn abs(rel: &str) -> String {
    std::env::temp_dir().join(rel).to_string_lossy().into_owned()
}

#[async_trait]
impl PhpDetectorPort for FakeDetector {
    async fn detect(&self) -> Vec<PhpInstallation> { self.0.clone() }
}

#[derive(Default)]
struct FakeDownloads { started: Mutex<Vec<(String, PathBuf)>>, after: Mutex<Option<PostInstall>>, active: bool }

#[async_trait]
impl DownloadProgressPort for FakeDownloads {
    async fn start_nginx_download(&self, _: &Path) {}
    async fn nginx_progress(&self) -> Option<DownloadProgress> { None }
    async fn start_php_download(&self, _: &str, _: &Path) {}
    async fn php_progress(&self, _: &str) -> Option<DownloadProgress> { None }
    async fn start_node_download(&self, _: &NodeVersion, _: &Path) {}
    async fn node_progress(&self, _: &NodeVersion) -> Option<DownloadProgress> { None }
    async fn start_composer_download(&self, release: &ComposerRelease, dest: &Path, after: PostInstall) {
        self.started.lock().push((release.version.clone(), dest.to_path_buf()));
        *self.after.lock() = Some(after);
    }
    async fn composer_progress(&self) -> Option<DownloadProgress> {
        self.active.then(|| DownloadProgress { state: TaskState::Downloading, message: String::new(), percent: 40, error: None })
    }
}

struct FakeIni(bool);

#[async_trait]
impl PhpIniStore for FakeIni {
    async fn read(&self, _: &str) -> Result<Option<PhpIniFile>, InfrastructureError> {
        Ok(self.0.then(|| PhpIniFile { path: "php.ini".into(), content: String::new() }))
    }
    async fn write(&self, _: &str, _: &str) -> Result<(), InfrastructureError> { Ok(()) }
}

fn install_uc(m: &Arc<FakeManager>, phps: Vec<PhpInstallation>, d: &Arc<FakeDownloads>) -> InstallComposerUseCase {
    InstallComposerUseCase::new(m.clone(), Arc::new(FakeReleases { offline: false }), Arc::new(FakeDetector(phps)), d.clone())
}

fn status_uc(m: &Arc<FakeManager>, phps: Vec<PhpInstallation>, offline: bool, managed_ini: bool) -> GetComposerStatusUseCase {
    GetComposerStatusUseCase::new(m.clone(), Arc::new(FakeReleases { offline }), Arc::new(FakeDetector(phps)), Arc::new(FakeIni(managed_ini)))
}

const ALL: [&str; 5] = ["core", "openssl", "mbstring", "curl", "zip"];

// ── Install ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn install_downloads_the_newest_release_and_writes_the_command_afterwards() {
    let m = FakeManager::with(None, 0, None, &ALL);
    let d = Arc::new(FakeDownloads::default());
    let state = install_uc(&m, vec![php("8.4", "8.4.10")], &d).execute("8.4").await.unwrap();
    assert_eq!(state, TaskState::Pending);
    assert_eq!(*d.started.lock(), vec![("2.11.2".to_string(), PathBuf::from("/base/composer"))]);
    assert!(m.command_php.lock().is_none(), "the command is written once the phar is there");
    let after = d.after.lock().take().unwrap();
    after().await.unwrap();
    assert_eq!(m.command_php.lock().as_deref(), Some(Path::new(&abs("php/8.4/php"))));
}

#[tokio::test]
async fn old_php_gets_the_lts_release() {
    let m = FakeManager::with(None, 0, None, &ALL);
    let d = Arc::new(FakeDownloads::default());
    install_uc(&m, vec![php("7.1", "7.1.33")], &d).execute("7.1").await.unwrap();
    assert_eq!(d.started.lock()[0].0, "2.2.32");
}

#[tokio::test]
async fn up_to_date_install_only_repairs_the_command() {
    let m = FakeManager::with(Some("2.11.2"), 70205, None, &ALL);
    let d = Arc::new(FakeDownloads::default());
    let state = install_uc(&m, vec![php("8.2", "8.2.20")], &d).execute("8.2").await.unwrap();
    assert_eq!(state, TaskState::Done);
    assert!(d.started.lock().is_empty());
    assert_eq!(m.command_php.lock().as_deref(), Some(Path::new(&abs("php/8.2/php"))));
}

#[tokio::test]
async fn install_needs_the_default_php() {
    let m = FakeManager::with(None, 0, None, &ALL);
    let d = Arc::new(FakeDownloads::default());
    let bare = PhpInstallation { major: "8.2".into(), version: "8.2.1".into(), binary_path: "php".into() };
    let err = install_uc(&m, vec![php("8.4", "8.4.1"), bare], &d).execute("8.2").await.unwrap_err();
    assert!(matches!(err, ApplicationError::Domain(DomainError::ComposerNeedsPhp(ref v)) if v == "8.2"), "{err}");
}

#[tokio::test]
async fn install_while_downloading_reports_the_running_task() {
    let m = FakeManager::with(None, 0, None, &ALL);
    let d = Arc::new(FakeDownloads { active: true, ..Default::default() });
    let state = install_uc(&m, vec![php("8.4", "8.4.1")], &d).execute("8.4").await.unwrap();
    assert_eq!(state, TaskState::Downloading);
    assert!(d.started.lock().is_empty());
}

// ── Status ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn status_reports_update_missing_extensions_and_stale_command() {
    let m = FakeManager::with(Some("2.10.0"), 70205, Some("/old/php"), &["core", "openssl", "curl"]);
    let view = status_uc(&m, vec![php("8.3", "8.3.5")], false, true).execute("8.3").await.unwrap();
    assert_eq!(view.latest.as_deref(), Some("2.11.2"));
    assert!(view.update_available);
    assert_eq!(view.missing_extensions, ["mbstring", "zip"]);
    assert!(view.can_enable_extensions);
    assert!(view.warnings.iter().any(|w| w.contains("Repair")), "{:?}", view.warnings);
}

#[tokio::test]
async fn status_warns_when_composer_is_too_new_for_the_default_php() {
    let m = FakeManager::with(Some("2.11.2"), 70205, Some(&abs("php/7.1/php")), &ALL);
    let view = status_uc(&m, vec![php("7.1", "7.1.33")], false, false).execute("7.1").await.unwrap();
    assert_eq!(view.latest.as_deref(), Some("2.2.32"));
    assert!(view.update_available);
    assert_eq!(view.warnings.len(), 1, "{:?}", view.warnings);
    assert!(view.warnings[0].contains("7.2.5"), "{}", view.warnings[0]);
}

#[tokio::test]
async fn status_works_offline_and_without_php() {
    let m = FakeManager::with(Some("2.11.2"), 70205, None, &ALL);
    let view = status_uc(&m, vec![], true, false).execute("8.2").await.unwrap();
    assert_eq!(view.latest, None);
    assert!(!view.update_available);
    assert!(view.missing_extensions.is_empty());
    assert!(view.warnings[0].contains("not installed"), "{:?}", view.warnings);
}

// ── Sync ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn sync_rewrites_only_an_installed_stale_command() {
    let detector = Arc::new(FakeDetector(vec![php("8.4", "8.4.1")]));
    let none = FakeManager::with(None, 0, None, &ALL);
    assert!(!SyncComposerCommandUseCase::new(none.clone(), detector.clone()).execute("8.4").await.unwrap());
    assert!(none.command_php.lock().is_none());

    let stale = FakeManager::with(Some("2.11.2"), 70205, Some("/old/php"), &ALL);
    let uc = SyncComposerCommandUseCase::new(stale.clone(), detector);
    assert!(uc.execute("8.4").await.unwrap());
    assert!(!uc.execute("8.4").await.unwrap(), "already up to date");
    assert!(!uc.execute("5.6").await.unwrap(), "that PHP is not installed");
    assert_eq!(stale.command_php.lock().as_deref(), Some(Path::new(&abs("php/8.4/php"))));
}
