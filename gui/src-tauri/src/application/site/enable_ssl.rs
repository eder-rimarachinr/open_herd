use std::{path::PathBuf, sync::Arc};

use crate::domain::{
    errors::ApplicationError,
    ports::{ssl::SslPort, web_server::WebServerPort},
    site::{repository::SiteRepository, value_objects::SiteId},
};

pub struct EnableSslUseCase {
    site_repo:  Arc<dyn SiteRepository>,
    ssl:        Arc<dyn SslPort>,
    web_server: Arc<dyn WebServerPort>,
    certs_dir:  PathBuf,
}

impl EnableSslUseCase {
    pub fn new(
        site_repo:  Arc<dyn SiteRepository>,
        ssl:        Arc<dyn SslPort>,
        web_server: Arc<dyn WebServerPort>,
        certs_dir:  PathBuf,
    ) -> Self {
        Self { site_repo, ssl, web_server, certs_dir }
    }

    /// Emite el certificado SSL y actualiza el sitio. Síncrono desde la perspectiva
    /// del caller — el handler HTTP lo lanza en `spawn_blocking` para no bloquear.
    pub async fn execute(&self, site_id: &str) -> Result<(), ApplicationError> {
        let id = SiteId::from_string(site_id);

        let mut site = self.site_repo
            .find_by_id(&id)
            .await?
            .ok_or_else(|| crate::domain::errors::DomainError::SiteNotFound(site_id.into()))?;

        // Regla de negocio: no emitir si ya hay cert activo
        if !site.can_enable_ssl() {
            return Err(crate::domain::errors::DomainError::SslAlreadyActive(
                site.domain.to_string(),
            ).into());
        }

        // Emitir certificado vía mkcert
        let (cert_path, key_path) = self.ssl
            .issue_certificate(site.domain.as_str(), &self.certs_dir)
            .await?;

        // Actualizar entidad (regla de negocio en la entidad)
        site.enable_ssl(cert_path, key_path)
            .map_err(crate::domain::errors::ApplicationError::Domain)?;

        // Persistir
        self.site_repo.save(&site).await?;

        // Regenerar vhost con SSL y recargar nginx
        self.web_server.create_vhost(&site).await?;
        if let Err(e) = self.web_server.reload().await {
            eprintln!("[warn] nginx reload failed after enable_ssl: {}", e);
        }

        Ok(())
    }
}
