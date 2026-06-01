use std::{path::PathBuf, sync::Arc};

use crate::{
    application::site::{
        bulk_add_sites::BulkAddSitesUseCase,
        create_site::CreateSiteUseCase,
        delete_site::DeleteSiteUseCase,
        disable_ssl::DisableSslUseCase,
        enable_ssl::EnableSslUseCase,
        refresh_site_config::RefreshSiteConfigUseCase,
        scan_sites::ScanSitesUseCase,
        update_site::UpdateSiteUseCase,
    },
    daemon::state::AppState,
    domain::ports::{dns::DnsPort, ssl::SslPort, web_server::WebServerPort},
    domain::site::repository::SiteRepository,
};

use super::{
    dns::hosts_adapter::HostsAdapter,
    nginx::adapter::NginxAdapter,
    persistence::json_site_repository::JsonSiteRepository,
    ssl::mkcert_adapter::MkcertAdapter,
};

/// Punto central de inyección de dependencias.
///
/// Implementa `Deref<Target=AppState>` para que los handlers legacy de `routes.rs`
/// sigan compilando sin modificación — `state.sites`, `state.config`, etc. se
/// resuelven automáticamente a través del deref.
///
/// Los nuevos handlers de `ports/http/site_handlers.rs` usan los use cases directamente.
pub struct AppContainer {
    /// Estado legacy — seguirá existiendo hasta que todos los handlers estén migrados.
    pub legacy: Arc<AppState>,

    // ── Puertos concretos (reutilizables por múltiples use cases) ─────────────
    pub site_repo:  Arc<dyn SiteRepository>,
    pub web_server: Arc<dyn WebServerPort>,
    pub dns:        Arc<dyn DnsPort>,
    pub ssl:        Arc<dyn SslPort>,

    // ── Casos de uso pre-construidos ─────────────────────────────────────────
    pub create_site_uc:        CreateSiteUseCase,
    pub delete_site_uc:        DeleteSiteUseCase,
    pub update_site_uc:        UpdateSiteUseCase,
    pub enable_ssl_uc:         EnableSslUseCase,
    pub disable_ssl_uc:        DisableSslUseCase,
    pub scan_sites_uc:         ScanSitesUseCase,
    pub bulk_add_sites_uc:     BulkAddSitesUseCase,
    pub refresh_site_config_uc: RefreshSiteConfigUseCase,
}

impl AppContainer {
    pub fn new(state: Arc<AppState>) -> Arc<Self> {
        let certs_dir = PathBuf::from(state.config.read().certs_dir.clone());
        let base_dir  = state.base_dir.clone();

        // Construir adaptadores concretos
        let site_repo:  Arc<dyn SiteRepository> =
            Arc::new(JsonSiteRepository::new(state.clone()));
        let web_server: Arc<dyn WebServerPort>  =
            Arc::new(NginxAdapter::new(state.clone()));
        let dns:        Arc<dyn DnsPort>        =
            Arc::new(HostsAdapter::new());
        let ssl:        Arc<dyn SslPort>        =
            Arc::new(MkcertAdapter::new(base_dir));

        // Construir use cases
        let create_site_uc = CreateSiteUseCase::new(
            site_repo.clone(), web_server.clone(), dns.clone(),
        );
        let delete_site_uc = DeleteSiteUseCase::new(
            site_repo.clone(), web_server.clone(), dns.clone(),
        );
        let update_site_uc = UpdateSiteUseCase::new(
            site_repo.clone(), web_server.clone(),
        );
        let enable_ssl_uc = EnableSslUseCase::new(
            site_repo.clone(), ssl.clone(), web_server.clone(), certs_dir.clone(),
        );
        let disable_ssl_uc = DisableSslUseCase::new(
            site_repo.clone(), ssl.clone(), web_server.clone(),
        );
        let scan_sites_uc = ScanSitesUseCase::new(
            site_repo.clone(), web_server.clone(), dns.clone(),
        );
        let bulk_add_sites_uc = BulkAddSitesUseCase::new(
            site_repo.clone(), web_server.clone(), dns.clone(),
        );
        let refresh_site_config_uc = RefreshSiteConfigUseCase::new(
            site_repo.clone(), web_server.clone(), dns.clone(),
        );

        Arc::new(Self {
            legacy: state,
            site_repo,
            web_server,
            dns,
            ssl,
            create_site_uc,
            delete_site_uc,
            update_site_uc,
            enable_ssl_uc,
            disable_ssl_uc,
            scan_sites_uc,
            bulk_add_sites_uc,
            refresh_site_config_uc,
        })
    }
}

/// Permite que los handlers legacy usen `state.sites`, `state.config`, etc.
/// sin cambios — el compilador hace el deref de `Arc<AppContainer>` →
/// `AppContainer` → `AppState` automáticamente.
impl std::ops::Deref for AppContainer {
    type Target = AppState;
    fn deref(&self) -> &Self::Target {
        &self.legacy
    }
}
