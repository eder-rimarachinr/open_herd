use crate::application::best_effort;
use std::sync::Arc;

use crate::domain::{
    errors::ApplicationError,
    ports::{dns::DnsPort, web_server::WebServerPort},
    site::{repository::SiteRepository, value_objects::SiteId},
};

pub struct DeleteSiteUseCase {
    site_repo:  Arc<dyn SiteRepository>,
    web_server: Arc<dyn WebServerPort>,
    dns:        Arc<dyn DnsPort>,
}

impl DeleteSiteUseCase {
    pub fn new(
        site_repo:  Arc<dyn SiteRepository>,
        web_server: Arc<dyn WebServerPort>,
        dns:        Arc<dyn DnsPort>,
    ) -> Self {
        Self { site_repo, web_server, dns }
    }

    pub async fn execute(&self, site_id: &str) -> Result<(), ApplicationError> {
        let id = SiteId::from_string(site_id);

        // 1. Cargar sitio (falla si no existe)
        let site = self.site_repo
            .find_by_id(&id)
            .await?
            .ok_or_else(|| crate::domain::errors::DomainError::SiteNotFound(site_id.into()))?;

        // 2. Eliminar de persistencia primero — si falla no tocamos nginx ni DNS
        self.site_repo.delete(&id).await?;

        // 3. Eliminar vhost (non-fatal)
        best_effort(self.web_server.remove_vhost(&site).await, format_args!("remove_vhost failed for {}", site.domain));

        // 4. Eliminar entrada DNS (non-fatal)
        best_effort(self.dns.remove_entry(site.domain.as_str()).await, format_args!("DNS remove_entry failed for {}", site.domain));

        // 5. Recargar nginx
        best_effort(self.web_server.reload().await, format_args!("nginx reload failed after delete_site"));

        Ok(())
    }
}
