use async_trait::async_trait;
use std::sync::Arc;
use crate::infrastructure::state::{save_sites, AppState};
use crate::domain::{errors::DomainError, site::{entity::Site, repository::SiteRepository, value_objects::{DomainName, SiteId}}};
use super::site_mapper;

pub struct JsonSiteRepository { state: Arc<AppState> }
impl JsonSiteRepository {
    pub fn new(state: Arc<AppState>) -> Self { Self { state } }
    fn certs_dir(&self) -> String { self.state.config.read().certs_dir.clone() }
}

#[async_trait]
impl SiteRepository for JsonSiteRepository {
    async fn find_by_id(&self, id: &SiteId) -> Result<Option<Site>, DomainError> {
        let certs = self.certs_dir(); let sites = self.state.sites.read();
        Ok(sites.get(id.as_str()).map(|l| site_mapper::to_domain(l, Some(&certs))))
    }
    async fn find_by_domain(&self, domain: &DomainName) -> Result<Option<Site>, DomainError> {
        let certs = self.certs_dir(); let sites = self.state.sites.read();
        Ok(sites.values().find(|s| s.domain == domain.as_str()).map(|l| site_mapper::to_domain(l, Some(&certs))))
    }
    async fn list_all(&self) -> Result<Vec<Site>, DomainError> {
        let certs = self.certs_dir(); let sites = self.state.sites.read();
        let mut result: Vec<Site> = sites.values().map(|l| site_mapper::to_domain(l, Some(&certs))).collect();
        result.sort_by(|a, b| a.domain.as_str().cmp(b.domain.as_str()));
        Ok(result)
    }
    async fn save(&self, site: &Site) -> Result<(), DomainError> {
        { let mut sites = self.state.sites.write(); sites.insert(site.id.to_string(), site_mapper::to_legacy(site)); }
        save_sites(&self.state).map_err(|e| DomainError::InvalidDomain(format!("Error guardando sites.json: {}", e)))
    }
    async fn delete(&self, id: &SiteId) -> Result<(), DomainError> {
        { let mut sites = self.state.sites.write(); if sites.remove(id.as_str()).is_none() { return Err(DomainError::SiteNotFound(id.to_string())); } }
        save_sites(&self.state).map_err(|e| DomainError::InvalidDomain(format!("Error guardando sites.json: {}", e)))
    }
}
