use std::{path::PathBuf, sync::{Arc, OnceLock}, time::Duration};

use crate::{
    application::{
        nginx::{
            download_nginx::DownloadNginxUseCase,
            reload_nginx::ReloadNginxUseCase,
            start_nginx::StartNginxUseCase,
            stop_nginx::StopNginxUseCase,
        },
        php::{
            detect_php::DetectPhpUseCase,
            install_php::InstallPhpUseCase,
            start_php::StartPhpUseCase,
            stop_php::StopPhpUseCase,
            update_php_ini::UpdatePhpIniUseCase,
        },
        services::{
            start_services::StartServicesUseCase,
            stop_services::StopServicesUseCase,
        },
        site::{
            bulk_add_sites::BulkAddSitesUseCase,
            create_site::CreateSiteUseCase,
            delete_site::DeleteSiteUseCase,
            disable_ssl::DisableSslUseCase,
            enable_ssl::EnableSslUseCase,
            refresh_site_config::RefreshSiteConfigUseCase,
            scan_sites::ScanSitesUseCase,
            update_site::UpdateSiteUseCase,
        },
    },
    infrastructure::state::AppState,
    domain::ports::{
        dns::DnsPort,
        download::DownloadProgressPort,
        logger::LoggerPort,
        process_manager::{PhpDetectorPort, PhpProcessPort, PhpVersionRepository},
        ssl::SslPort,
        ssl_task::SslTaskPort,
        web_server::WebServerPort,
    },
    domain::site::repository::SiteRepository,
};

use crate::infrastructure::{
    dns::hosts_adapter::HostsAdapter,
    download::tracker::DownloadTracker,
    nginx::adapter::NginxAdapter,
    persistence::json_site_repository::JsonSiteRepository,
    logging::InMemoryLogger,
    php::{adapter::PhpProcessAdapter, detector::SystemPhpDetector, process as php_process, version_repository::InMemoryPhpVersionRepository},
    ssl::mkcert_adapter::MkcertAdapter,
    ssl::task_tracker::InMemorySslTaskTracker,
};

pub struct AppContainer {
    /// Estado legacy — existirá hasta que todos los handlers estén migrados.
    pub legacy: Arc<AppState>,

    // ── Puertos concretos ────────────────────────────────────────────────────
    pub site_repo:  Arc<dyn SiteRepository>,
    pub web_server: Arc<dyn WebServerPort>,
    pub dns:        Arc<dyn DnsPort>,
    pub ssl:        Arc<dyn SslPort>,
    /// Renombrado con sufijo `_port` para evitar shadowing del campo `php_proc`
    /// de `AppState` que los handlers legacy acceden a través del Deref.
    pub php_process_port: Arc<dyn PhpProcessPort>,
    pub php_detector:     Arc<dyn PhpDetectorPort>,
    pub php_version_repo: Arc<dyn PhpVersionRepository>,
    pub downloads: Arc<dyn DownloadProgressPort>,
    pub ssl_tasks: Arc<dyn SslTaskPort>,
    pub logger: Arc<dyn LoggerPort>,

    // ── Casos de uso — Sites ─────────────────────────────────────────────────
    pub create_site_uc:         CreateSiteUseCase,
    pub delete_site_uc:         DeleteSiteUseCase,
    pub update_site_uc:         UpdateSiteUseCase,
    pub enable_ssl_uc:          EnableSslUseCase,
    pub disable_ssl_uc:         DisableSslUseCase,
    pub scan_sites_uc:          ScanSitesUseCase,
    pub bulk_add_sites_uc:      BulkAddSitesUseCase,
    pub refresh_site_config_uc: RefreshSiteConfigUseCase,

    // ── Casos de uso — PHP ───────────────────────────────────────────────────
    pub detect_php_uc: DetectPhpUseCase,
    pub start_php_uc:  StartPhpUseCase,
    pub stop_php_uc:   StopPhpUseCase,

    // ── Casos de uso — PHP (extra) ───────────────────────────────────────────
    pub install_php_uc:     InstallPhpUseCase,
    pub update_php_ini_uc:  UpdatePhpIniUseCase,

    // ── Casos de uso — Nginx ─────────────────────────────────────────────────
    pub start_nginx_uc:    StartNginxUseCase,
    pub stop_nginx_uc:     StopNginxUseCase,
    pub reload_nginx_uc:   ReloadNginxUseCase,
    pub download_nginx_uc: DownloadNginxUseCase,

    // ── Casos de uso — Services ──────────────────────────────────────────────
    pub start_services_uc: StartServicesUseCase,
    pub stop_services_uc:  StopServicesUseCase,

    /// Cómo terminar la app tras un apagado ordenado. `lib.rs` lo fija con
    /// `AppHandle::exit` para que Tauri limpie (icono de bandeja, WebView);
    /// sin hook (tests, antes de `setup`) se usa `std::process::exit`.
    exit_hook: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

/// Adaptadores que modifican la máquina fuera de `base_dir` (el archivo hosts
/// del sistema, el almacén de confianza del SO vía `mkcert -install`). Los tests
/// los sustituyen por dobles; `None` usa el adaptador real.
#[derive(Default)]
pub struct SystemAdapters {
    pub dns: Option<Arc<dyn DnsPort>>,
    pub ssl: Option<Arc<dyn SslPort>>,
}

impl AppContainer {
    pub fn new(state: Arc<AppState>) -> Arc<Self> {
        Self::with_adapters(state, SystemAdapters::default())
    }

    pub fn with_adapters(state: Arc<AppState>, system: SystemAdapters) -> Arc<Self> {
        let certs_dir = PathBuf::from(state.config.read().certs_dir.clone());
        let base_dir  = state.base_dir.clone();

        // ── Adaptadores ──────────────────────────────────────────────────────
        let site_repo:    Arc<dyn SiteRepository>  = Arc::new(JsonSiteRepository::new(state.clone()));
        let logger: Arc<dyn LoggerPort> = InMemoryLogger::new();
        for warning in &state.load_warnings { logger.log(warning.clone()); }
        let web_server:   Arc<dyn WebServerPort>   = Arc::new(NginxAdapter::new(state.clone(), logger.clone()));
        let dns:          Arc<dyn DnsPort>          = system.dns.unwrap_or_else(|| Arc::new(HostsAdapter::new()));
        let ssl:          Arc<dyn SslPort>          = system.ssl.unwrap_or_else(|| Arc::new(MkcertAdapter::new(base_dir)));
        let php_process_port: Arc<dyn PhpProcessPort>  = Arc::new(PhpProcessAdapter::new(state.clone(), logger.clone()));
        let php_detector: Arc<dyn PhpDetectorPort> = Arc::new(SystemPhpDetector::new(state.clone()));
        let php_version_repo: Arc<dyn PhpVersionRepository> = InMemoryPhpVersionRepository::new();
        let downloads: Arc<dyn DownloadProgressPort> = DownloadTracker::new();
        let ssl_tasks: Arc<dyn SslTaskPort> = InMemorySslTaskTracker::new();

        // ── Use cases — Sites ────────────────────────────────────────────────
        let create_site_uc = CreateSiteUseCase::new(site_repo.clone(), web_server.clone(), dns.clone());
        let delete_site_uc = DeleteSiteUseCase::new(site_repo.clone(), web_server.clone(), dns.clone());
        let update_site_uc = UpdateSiteUseCase::new(site_repo.clone(), web_server.clone());
        let enable_ssl_uc  = EnableSslUseCase::new(site_repo.clone(), ssl.clone(), web_server.clone(), certs_dir.clone());
        let disable_ssl_uc = DisableSslUseCase::new(site_repo.clone(), ssl.clone(), web_server.clone(), certs_dir);
        let scan_sites_uc  = ScanSitesUseCase::new(site_repo.clone(), web_server.clone(), dns.clone());
        let bulk_add_sites_uc      = BulkAddSitesUseCase::new(site_repo.clone(), web_server.clone(), dns.clone());
        let refresh_site_config_uc = RefreshSiteConfigUseCase::new(site_repo.clone(), web_server.clone(), dns.clone());

        // ── Use cases — PHP ──────────────────────────────────────────────────
        let detect_php_uc = DetectPhpUseCase::new(php_detector.clone(), php_version_repo.clone());
        let start_php_uc  = StartPhpUseCase::new(php_detector.clone(), php_process_port.clone(), php_version_repo.clone());
        let stop_php_uc   = StopPhpUseCase::new(php_process_port.clone());

        // ── Use cases — PHP (extra) ──────────────────────────────────────────
        let install_php_uc    = InstallPhpUseCase::new(downloads.clone());
        let update_php_ini_uc = UpdatePhpIniUseCase::new(
            php_process_port.clone(), php_detector.clone(), web_server.clone(),
        );

        // ── Use cases — Nginx ────────────────────────────────────────────────
        let start_nginx_uc    = StartNginxUseCase::new(web_server.clone());
        let stop_nginx_uc     = StopNginxUseCase::new(web_server.clone());
        let reload_nginx_uc   = ReloadNginxUseCase::new(web_server.clone());
        let download_nginx_uc = DownloadNginxUseCase::new(downloads.clone());

        // ── Use cases — Services ─────────────────────────────────────────────
        let start_services_uc = StartServicesUseCase::new(php_detector.clone(), php_process_port.clone(), web_server.clone(), php_version_repo.clone());
        let stop_services_uc  = StopServicesUseCase::new(php_process_port.clone(), web_server.clone());

        Arc::new(Self {
            legacy: state,
            site_repo, web_server, dns, ssl, php_process_port, php_detector, php_version_repo, downloads, ssl_tasks, logger,
            create_site_uc, delete_site_uc, update_site_uc,
            enable_ssl_uc, disable_ssl_uc, scan_sites_uc,
            bulk_add_sites_uc, refresh_site_config_uc,
            detect_php_uc, start_php_uc, stop_php_uc,
            install_php_uc, update_php_ini_uc,
            start_nginx_uc, stop_nginx_uc, reload_nginx_uc, download_nginx_uc,
            start_services_uc, stop_services_uc,
            exit_hook: OnceLock::new(),
        })
    }

    pub fn set_exit_hook(&self, exit: impl Fn() + Send + Sync + 'static) {
        let _ = self.exit_hook.set(Box::new(exit));
    }

    /// Termina la app; ver `exit_hook`.
    pub fn exit_app(&self) {
        match self.exit_hook.get() {
            Some(exit) => exit(),
            None => std::process::exit(0),
        }
    }

    /// Tareas de mantenimiento que viven en el runtime del daemon. Debe
    /// llamarse una vez, desde dentro de ese runtime.
    pub fn spawn_background_tasks(&self) {
        let php    = self.legacy.php_proc.clone();
        let logger = self.logger.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(2));
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tick.tick().await;
                let (php, logger) = (php.clone(), logger.clone());
                // supervise() may spawn processes: keep it off the async workers.
                let _ = tokio::task::spawn_blocking(move || php_process::supervise(&php, logger.as_ref())).await;
            }
        });
    }
}

impl std::ops::Deref for AppContainer {
    type Target = AppState;
    fn deref(&self) -> &Self::Target { &self.legacy }
}
