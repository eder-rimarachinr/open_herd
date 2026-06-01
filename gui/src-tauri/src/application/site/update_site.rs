use std::sync::Arc;

use crate::domain::{
    errors::ApplicationError,
    ports::web_server::WebServerPort,
    site::{
        entity::Site,
        repository::SiteRepository,
        value_objects::{PhpVersion, SiteId},
    },
};

pub struct UpdateSiteCommand {
    pub site_id:     String,
    pub php_version: Option<String>,
    pub active:      Option<bool>,
}

pub struct UpdateSiteUseCase {
    site_repo:  Arc<dyn SiteRepository>,
    web_server: Arc<dyn WebServerPort>,
}

impl UpdateSiteUseCase {
    pub fn new(site_repo: Arc<dyn SiteRepository>, web_server: Arc<dyn WebServerPort>) -> Self {
        Self { site_repo, web_server }
    }

    pub async fn execute(&self, cmd: UpdateSiteCommand) -> Result<Site, ApplicationError> {
        let id = SiteId::from_string(&cmd.site_id);

        let mut site = self.site_repo
            .find_by_id(&id)
            .await?
            .ok_or_else(|| crate::domain::errors::DomainError::SiteNotFound(cmd.site_id.clone()))?;

        // Aplicar cambios opcionales
        if let Some(ver_str) = cmd.php_version {
            if let Some(v) = PhpVersion::parse(&ver_str) {
                site.set_php_version(v);
            }
        }
        if let Some(active) = cmd.active {
            site.active = active;
            site.updated_at = chrono::Utc::now();
        }

        // Persistir y regenerar config de nginx
        self.site_repo.save(&site).await?;
        if let Err(e) = self.web_server.create_vhost(&site).await {
            eprintln!("[warn] create_vhost failed after update_site: {}", e);
        }
        if let Err(e) = self.web_server.reload().await {
            eprintln!("[warn] nginx reload failed after update_site: {}", e);
        }

        Ok(site)
    }
}
