use std::sync::Arc;

use crate::domain::{
    errors::ApplicationError,
    ports::{dns::DnsPort, web_server::WebServerPort},
    site::{entity::Site, repository::SiteRepository, value_objects::SiteId},
};
use super::project_type_detector;

pub struct RefreshSiteConfigUseCase {
    site_repo:  Arc<dyn SiteRepository>,
    web_server: Arc<dyn WebServerPort>,
    dns:        Arc<dyn DnsPort>,
}

impl RefreshSiteConfigUseCase {
    pub fn new(
        site_repo:  Arc<dyn SiteRepository>,
        web_server: Arc<dyn WebServerPort>,
        dns:        Arc<dyn DnsPort>,
    ) -> Self {
        Self { site_repo, web_server, dns }
    }

    /// Re-detecta el tipo de proyecto, regenera el vhost de nginx y confirma la entrada DNS.
    pub async fn execute(&self, site_id: &str) -> Result<Site, ApplicationError> {
        let id = SiteId::from_string(site_id);

        let mut site = self.site_repo
            .find_by_id(&id)
            .await?
            .ok_or_else(|| crate::domain::errors::DomainError::SiteNotFound(site_id.into()))?;

        // Re-detectar tipo de proyecto desde disco
        let detected = project_type_detector::detect(site.path.as_path().to_string_lossy().as_ref());
        site.set_project_type(detected);

        // Persistir cambios
        self.site_repo.save(&site).await?;

        // Regenerar config nginx con el nuevo tipo
        self.web_server.create_vhost(&site).await?;

        // Asegurar entrada DNS (idempotente)
        if let Err(e) = self.dns.add_entry(site.domain.as_str()).await {
            eprintln!("[refresh] DNS error for {}: {}", site.domain, e);
        }

        if let Err(e) = self.web_server.reload().await {
            eprintln!("[refresh] nginx reload error: {}", e);
        }

        Ok(site)
    }
}
