use std::sync::Arc;

use crate::domain::{
    errors::ApplicationError,
    ports::{ssl::SslPort, web_server::WebServerPort},
    site::{repository::SiteRepository, value_objects::SiteId},
};

pub struct DisableSslUseCase {
    site_repo:  Arc<dyn SiteRepository>,
    ssl:        Arc<dyn SslPort>,
    web_server: Arc<dyn WebServerPort>,
}

impl DisableSslUseCase {
    pub fn new(
        site_repo:  Arc<dyn SiteRepository>,
        ssl:        Arc<dyn SslPort>,
        web_server: Arc<dyn WebServerPort>,
    ) -> Self {
        Self { site_repo, ssl, web_server }
    }

    pub async fn execute(&self, site_id: &str) -> Result<(), ApplicationError> {
        let id = SiteId::from_string(site_id);

        let mut site = self.site_repo
            .find_by_id(&id)
            .await?
            .ok_or_else(|| crate::domain::errors::DomainError::SiteNotFound(site_id.into()))?;

        // Idempotente: si ya está deshabilitado no hay nada que hacer
        if !site.ssl.is_enabled() {
            return Ok(());
        }

        // Revocar archivos de certificado
        if let Err(e) = self.ssl.revoke_certificate(site.domain.as_str()).await {
            eprintln!("[warn] revoke_certificate failed for {}: {}", site.domain, e);
        }

        // Actualizar entidad
        site.disable_ssl();

        // Persistir y regenerar vhost HTTP-only
        self.site_repo.save(&site).await?;
        self.web_server.create_vhost(&site).await?;
        if let Err(e) = self.web_server.reload().await {
            eprintln!("[warn] nginx reload failed after disable_ssl: {}", e);
        }

        Ok(())
    }
}
