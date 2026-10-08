use crate::application::best_effort;
use std::sync::Arc;

use crate::domain::{
    errors::ApplicationError,
    ports::{dns::DnsPort, web_server::WebServerPort},
    site::{
        entity::Site,
        repository::SiteRepository,
        value_objects::{DomainName, SitePath},
    },
};
use super::project_type_detector;

pub struct CreateSiteCommand {
    pub domain: String,
    pub path: String,
    /// Versión de PHP por defecto tomada de la configuración.
    pub default_php: Option<String>,
}

/// Orquesta la creación de un sitio: valida entradas, verifica duplicados,
/// persiste, genera el vhost de nginx y registra la entrada DNS.
///
/// Principio: falla atómico — si nginx falla, el sitio no queda guardado.
pub struct CreateSiteUseCase {
    site_repo:  Arc<dyn SiteRepository>,
    web_server: Arc<dyn WebServerPort>,
    dns:        Arc<dyn DnsPort>,
}

impl CreateSiteUseCase {
    pub fn new(
        site_repo:  Arc<dyn SiteRepository>,
        web_server: Arc<dyn WebServerPort>,
        dns:        Arc<dyn DnsPort>,
    ) -> Self {
        Self { site_repo, web_server, dns }
    }

    pub async fn execute(&self, cmd: CreateSiteCommand) -> Result<Site, ApplicationError> {
        // 1. Validar value objects (reglas de dominio)
        let domain = DomainName::new(&cmd.domain)?;
        let path   = SitePath::new(&cmd.path)?;

        // 2. Verificar que el dominio no esté ya registrado
        if self.site_repo.find_by_domain(&domain).await?.is_some() {
            return Err(crate::domain::errors::DomainError::DomainAlreadyExists(
                cmd.domain.clone(),
            ).into());
        }

        // 3. Construir la entidad
        let mut site = Site::new(cmd.domain.clone(), domain, path);
        site.project_type = project_type_detector::detect(&cmd.path);
        if let Some(ref ver) = cmd.default_php {
            if let Some(v) = crate::domain::site::value_objects::PhpVersion::parse(ver) {
                site.php_version = Some(v);
            }
        }

        // 4. Crear vhost ANTES de persistir (fail-fast: si nginx falla, no guardamos)
        self.web_server.create_vhost(&site).await?;

        // 5. Persistir
        self.site_repo.save(&site).await?;

        // 6. DNS (no-fatal: loguear pero no revertir)
        best_effort(self.dns.add_entry(site.domain.as_str()).await, format_args!("DNS add_entry failed for {}", site.domain));

        // 7. Recargar nginx
        best_effort(self.web_server.reload().await, format_args!("nginx reload failed after create_site"));

        Ok(site)
    }
}
