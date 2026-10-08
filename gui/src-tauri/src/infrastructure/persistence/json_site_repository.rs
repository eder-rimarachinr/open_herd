use async_trait::async_trait;
use std::sync::Arc;
use crate::infrastructure::{dto::Site as LegacySite, state::AppState};
use crate::domain::{errors::DomainError, site::{entity::Site, repository::SiteRepository, value_objects::{DomainName, SiteId}}};
use super::site_mapper;

pub struct JsonSiteRepository { state: Arc<AppState> }
impl JsonSiteRepository {
    pub fn new(state: Arc<AppState>) -> Self { Self { state } }
    fn certs_dir(&self) -> String { self.state.config.read().certs_dir.clone() }

    /// A stored record that fails validation is a data problem, not a user
    /// error — report it as `Persistence` so the API answers 500 with context.
    fn map_stored(legacy: &LegacySite, certs: &str) -> Result<Site, DomainError> {
        site_mapper::to_domain(legacy, Some(certs)).map_err(|e| DomainError::Persistence(
            format!("el registro guardado de {} está dañado: {e}", legacy.domain),
        ))
    }
}

fn write_failed(e: &anyhow::Error) -> DomainError {
    DomainError::Persistence(format!("no se pudo guardar sites.json: {e}"))
}

#[async_trait]
impl SiteRepository for JsonSiteRepository {
    async fn find_by_id(&self, id: &SiteId) -> Result<Option<Site>, DomainError> {
        let certs = self.certs_dir(); let sites = self.state.sites.read();
        sites.get(id.as_str()).map(|l| Self::map_stored(l, &certs)).transpose()
    }
    async fn find_by_domain(&self, domain: &DomainName) -> Result<Option<Site>, DomainError> {
        let certs = self.certs_dir(); let sites = self.state.sites.read();
        sites.values().find(|s| s.domain == domain.as_str()).map(|l| Self::map_stored(l, &certs)).transpose()
    }
    /// Records that fail validation are skipped here (they were reported once
    /// at load time) so one bad entry does not hide every other site.
    async fn list_all(&self) -> Result<Vec<Site>, DomainError> {
        let certs = self.certs_dir(); let sites = self.state.sites.read();
        let mut result: Vec<Site> = sites.values().filter_map(|l| site_mapper::to_domain(l, Some(&certs)).ok()).collect();
        result.sort_by(|a, b| a.domain.as_str().cmp(b.domain.as_str()));
        Ok(result)
    }
    async fn save(&self, site: &Site) -> Result<(), DomainError> {
        let legacy = site_mapper::to_legacy(site);
        self.state.update_sites(|sites| { sites.insert(legacy.id.clone(), legacy); true })
            .map(|_| ())
            .map_err(|e| write_failed(&e))
    }
    async fn delete(&self, id: &SiteId) -> Result<(), DomainError> {
        let removed = self.state.update_sites(|sites| sites.remove(id.as_str()).is_some())
            .map_err(|e| write_failed(&e))?;
        if removed { Ok(()) } else { Err(DomainError::SiteNotFound(id.to_string())) }
    }
}
