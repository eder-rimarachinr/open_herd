use crate::application::best_effort;
use std::sync::Arc;

use crate::domain::{
    errors::ApplicationError,
    ports::{dns::DnsPort, web_server::WebServerPort},
    site::{entity::Site, repository::SiteRepository, value_objects::{DomainName, SitePath}},
};
use super::{project_type_detector, register_new_site};

pub struct BulkSiteItem {
    pub domain: String,
    pub path:   String,
}

/// Añade múltiples sitios en una sola operación. Los items inválidos o duplicados
/// se saltan silenciosamente — el caller recibe la lista completa de sitios resultante.
pub struct BulkAddSitesUseCase {
    site_repo:  Arc<dyn SiteRepository>,
    web_server: Arc<dyn WebServerPort>,
    dns:        Arc<dyn DnsPort>,
}

impl BulkAddSitesUseCase {
    pub fn new(
        site_repo:  Arc<dyn SiteRepository>,
        web_server: Arc<dyn WebServerPort>,
        dns:        Arc<dyn DnsPort>,
    ) -> Self {
        Self { site_repo, web_server, dns }
    }

    pub async fn execute(
        &self,
        items: Vec<BulkSiteItem>,
        default_php: Option<String>,
    ) -> Result<Vec<Site>, ApplicationError> {
        let existing = self.site_repo.list_all().await?;
        let existing_domains: std::collections::HashSet<String> =
            existing.iter().map(|s| s.domain.to_string()).collect();

        let mut any_added = false;

        for item in items {
            if item.domain.is_empty() || item.path.is_empty() { continue; }

            // Validación silenciosa — bulk no aborta por inputs malos
            let domain = match DomainName::new(&item.domain) {
                Ok(d) => d,
                Err(_) => continue,
            };
            let site_path = match SitePath::new(&item.path) {
                Ok(p) => p,
                Err(_) => continue,
            };

            if existing_domains.contains(item.domain.as_str()) { continue; }

            let mut site = crate::domain::site::entity::Site::new(
                item.domain.clone(), domain, site_path,
            );
            site.project_type = project_type_detector::detect(&item.path);
            if let Some(ref ver) = default_php {
                if let Some(v) = crate::domain::site::value_objects::PhpVersion::parse(ver) {
                    site.php_version = Some(v);
                }
            }

            if !register_new_site(self.site_repo.as_ref(), self.web_server.as_ref(), self.dns.as_ref(), &site).await {
                continue;
            }

            any_added = true;
        }

        if any_added {
            best_effort(self.web_server.reload().await, format_args!("nginx reload error"));
        }

        self.site_repo.list_all().await.map_err(ApplicationError::from)
    }
}
