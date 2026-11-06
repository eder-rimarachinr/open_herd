#![allow(clippy::unwrap_used, clippy::expect_used)]
// Each integration test crate compiles this module and uses a different subset.
#![allow(dead_code)]

use async_trait::async_trait;
use axum_test::TestServer;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

use open_herd_lib::{
    domain::{
        errors::InfrastructureError,
        node::{NodeReleaseInfo, NodeVersion},
        ports::{
            dns::DnsPort, file_manager::FileManagerPort, ssl::SslPort, web_server::WebServerPort,
            node::{NodeEnvironment, NodeInstall, NodeManagerMode, NodeManagerPort, NodeReleasesPort},
        },
        site::entity::Site,
    },
    infrastructure::{
        config::Config,
        container::{AppContainer, SystemAdapters},
        state::AppState,
    },
    ports::http::server::build_router,
};

pub fn make_state(tmp: &TempDir) -> Arc<AppState> {
    let base = tmp.path().to_path_buf();
    std::fs::create_dir_all(&base).unwrap();
    let config = Config {
        api_addr:        "127.0.0.1:0".into(),
        base_dir:        base.to_string_lossy().into(),
        nginx_dir:       base.join("nginx").to_string_lossy().into(),
        php_dir:         base.join("php").to_string_lossy().into(),
        certs_dir:       base.join("certs").to_string_lossy().into(),
        logs_dir:        base.join("logs").to_string_lossy().into(),
        http_port:       8080,
        https_port:      8443,
        scanned_dirs:    vec![],
        default_php:     "8.2".into(),
        custom_php_dirs: vec![],
        os:              "windows".into(),
    };
    AppState::new(base, config)
}

/// Never touches the system hosts file.
struct NoopDns;

#[async_trait]
impl DnsPort for NoopDns {
    async fn add_entry(&self, _domain: &str) -> Result<(), InfrastructureError> { Ok(()) }
    async fn remove_entry(&self, _domain: &str) -> Result<(), InfrastructureError> { Ok(()) }
}

/// Never downloads mkcert nor runs `mkcert -install` (which would add a root
/// CA to the developer's trust store); reports where the cert would live.
struct FakeSsl;

#[async_trait]
impl SslPort for FakeSsl {
    async fn issue_certificate(&self, domain: &str, certs_dir: &Path) -> Result<(String, String), InfrastructureError> {
        let cert = certs_dir.join(format!("{domain}.pem"));
        let key  = certs_dir.join(format!("{domain}-key.pem"));
        Ok((cert.to_string_lossy().into_owned(), key.to_string_lossy().into_owned()))
    }
    async fn revoke_certificate(&self, _domain: &str, _certs_dir: &Path) -> Result<(), InfrastructureError> { Ok(()) }
}

/// Never opens Explorer / Finder on the developer's desktop; records the
/// folders it was asked to show.
#[derive(Default)]
pub struct RecordingFileManager {
    pub opened: Mutex<Vec<PathBuf>>,
}

#[async_trait]
impl FileManagerPort for RecordingFileManager {
    async fn open_folder(&self, path: &Path) -> Result<(), InfrastructureError> {
        self.opened.lock().unwrap().push(path.to_path_buf());
        Ok(())
    }
}

/// Never creates junctions or edits the user PATH: an in-memory list of
/// installs and the active one.
pub struct FakeNodeManager {
    pub installed: Mutex<Vec<NodeVersion>>,
    pub active: Mutex<Option<NodeVersion>>,
}

impl FakeNodeManager {
    pub fn new(installed: &[&str], active: Option<&str>) -> Arc<Self> {
        let parse = |s: &str| NodeVersion::parse(s).unwrap();
        Arc::new(Self {
            installed: Mutex::new(installed.iter().map(|s| parse(s)).collect()),
            active: Mutex::new(active.map(parse)),
        })
    }
}

#[async_trait]
impl NodeManagerPort for FakeNodeManager {
    async fn environment(&self) -> Result<NodeEnvironment, InfrastructureError> {
        Ok(NodeEnvironment {
            mode: NodeManagerMode::NvmWindows,
            store_dir: PathBuf::from(r"C:\nvm"),
            link_path: PathBuf::from(r"C:\nvm4w\nodejs"),
            installed: self.installed.lock().unwrap().iter()
                .map(|v| NodeInstall { version: v.clone(), path: PathBuf::from(r"C:\nvm").join(v.dir_name()) })
                .collect(),
            active: self.active.lock().unwrap().clone(),
            warnings: vec![],
        })
    }
    async fn activate(&self, version: &NodeVersion) -> Result<(), InfrastructureError> {
        *self.active.lock().unwrap() = Some(version.clone());
        Ok(())
    }
    async fn uninstall(&self, version: &NodeVersion) -> Result<(), InfrastructureError> {
        self.installed.lock().unwrap().retain(|v| v != version);
        Ok(())
    }
}

/// Never goes to nodejs.org: a fixed release list, or "offline".
pub struct FakeNodeReleases { pub fail: bool }

#[async_trait]
impl NodeReleasesPort for FakeNodeReleases {
    async fn releases(&self, _refresh: bool) -> Result<Vec<NodeReleaseInfo>, InfrastructureError> {
        if self.fail { return Err(InfrastructureError::DownloadFailed("nodejs.org unreachable".into())); }
        let today = chrono::Local::now().date_naive();
        let rel = |v: &str, date: chrono::NaiveDate, lts: &str| NodeReleaseInfo {
            version: NodeVersion::parse(v).unwrap(), date, lts: Some(lts.into()), security: false, has_build: true,
        };
        // 10.x: an LTS line nodejs.org stopped releasing years ago.
        let long_ago = chrono::NaiveDate::from_ymd_opt(2021, 4, 6).unwrap_or(today);
        Ok(vec![rel("24.21.0", today, "Krypton"), rel("10.24.1", long_ago, "Dubnium")])
    }
}

/// Real container over a temp `base_dir`, with the system-touching adapters
/// (hosts file, trust store, file manager, Node junction/PATH, nodejs.org)
/// replaced by fakes.
pub fn make_container(tmp: &TempDir) -> Arc<AppContainer> {
    make_container_with(tmp, Arc::new(RecordingFileManager::default()), None)
}

/// `web_server: None` keeps the real nginx adapter: it only writes vhost files
/// under the temp dir, since tests never start nginx.
fn make_container_with(
    tmp: &TempDir,
    file_manager: Arc<RecordingFileManager>,
    web_server: Option<Arc<dyn WebServerPort>>,
) -> Arc<AppContainer> {
    make_container_full(tmp, file_manager, web_server, FakeNodeManager::new(&[], None), Arc::new(FakeNodeReleases { fail: false }))
}

fn make_container_full(
    tmp: &TempDir,
    file_manager: Arc<RecordingFileManager>,
    web_server: Option<Arc<dyn WebServerPort>>,
    node_manager: Arc<dyn NodeManagerPort>,
    node_releases: Arc<dyn NodeReleasesPort>,
) -> Arc<AppContainer> {
    AppContainer::with_adapters(make_state(tmp), SystemAdapters {
        dns: Some(Arc::new(NoopDns)),
        ssl: Some(Arc::new(FakeSsl)),
        file_manager: Some(file_manager),
        web_server,
        node_manager: Some(node_manager),
        node_releases: Some(node_releases),
    })
}

/// Server whose Node manager / release list are the given doubles.
pub fn make_server_with_node(tmp: &TempDir, node: Arc<FakeNodeManager>, releases_offline: bool) -> TestServer {
    let container = make_container_full(
        tmp, Arc::new(RecordingFileManager::default()), None, node,
        Arc::new(FakeNodeReleases { fail: releases_offline }),
    );
    TestServer::new(build_router(container)).unwrap()
}

pub fn make_server(tmp: &TempDir) -> TestServer {
    TestServer::new(build_router(make_container(tmp))).unwrap()
}

/// Like `make_server`, plus the file-manager double to assert on.
pub fn make_server_with_file_manager(tmp: &TempDir) -> (TestServer, Arc<RecordingFileManager>) {
    let file_manager = Arc::new(RecordingFileManager::default());
    let server = TestServer::new(build_router(make_container_with(tmp, file_manager.clone(), None))).unwrap();
    (server, file_manager)
}

/// A web server that reports itself running and records stop/start. The next
/// `fail_starts` starts fail, like nginx finding its port already taken.
#[derive(Default)]
pub struct FakeWebServer {
    pub running: bool,
    pub fail_starts: Mutex<u32>,
    pub calls: Mutex<Vec<&'static str>>,
}

#[async_trait]
impl WebServerPort for FakeWebServer {
    async fn create_vhost(&self, _: &Site) -> Result<(), InfrastructureError> { Ok(()) }
    async fn remove_vhost(&self, _: &Site) -> Result<(), InfrastructureError> { Ok(()) }
    async fn reload(&self) -> Result<(), InfrastructureError> { Ok(()) }
    async fn is_running(&self) -> bool { self.running }
    async fn stop(&self) -> Result<(), InfrastructureError> { self.calls.lock().unwrap().push("stop"); Ok(()) }
    async fn start(&self) -> Result<(), InfrastructureError> {
        self.calls.lock().unwrap().push("start");
        let mut failing = self.fail_starts.lock().unwrap();
        if *failing > 0 {
            *failing -= 1;
            return Err(InfrastructureError::ProcessFailed("bind() to 0.0.0.0:8081 failed (10048: address in use)".into()));
        }
        Ok(())
    }
}

pub fn make_server_with_web_server(tmp: &TempDir, web: Arc<FakeWebServer>) -> TestServer {
    let container = make_container_with(tmp, Arc::new(RecordingFileManager::default()), Some(web));
    TestServer::new(build_router(container)).unwrap()
}
