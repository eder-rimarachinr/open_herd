use crate::application::best_effort;
use std::sync::Arc;

use crate::domain::{
    errors::ApplicationError,
    ports::web_server::WebServerPort,
    site::{
        entity::Site,
        repository::SiteRepository,
        value_objects::{PhpVersion, SiteId, SitePath},
    },
};
use super::project_type_detector;

pub struct UpdateSiteCommand {
    pub site_id:     String,
    pub php_version: Option<String>,
    pub active:      Option<bool>,
    /// New document folder. Validated like on create; the project type is
    /// re-detected there. Taken as given: no nested-project lookup, since the
    /// user picked this exact folder.
    pub path:        Option<String>,
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

        // Validate before touching anything, so a bad path changes nothing.
        let new_path = cmd.path.as_deref().map(SitePath::new).transpose()?;

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

        let relocated = new_path.is_some();
        if let Some(path) = new_path {
            let project_type = project_type_detector::detect(&path.to_string());
            site.relocate(path, project_type);
            // Like create: a vhost that can't be written for the new root
            // leaves the stored site untouched.
            self.web_server.create_vhost(&site).await?;
        }

        // Persistir y regenerar config de nginx
        self.site_repo.save(&site).await?;
        if !relocated {
            best_effort(self.web_server.create_vhost(&site).await, format_args!("create_vhost failed after update_site"));
        }
        best_effort(self.web_server.reload().await, format_args!("nginx reload failed after update_site"));

        Ok(site)
    }
}
