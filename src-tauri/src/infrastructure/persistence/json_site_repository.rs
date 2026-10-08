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
        self.update_off_runtime(move |sites| { sites.insert(legacy.id.clone(), legacy); true })
            .await
            .map(|_| ())
    }
    async fn delete(&self, id: &SiteId) -> Result<(), DomainError> {
        let key = id.as_str().to_owned();
        let removed = self.update_off_runtime(move |sites| sites.remove(&key).is_some()).await?;
        if removed { Ok(()) } else { Err(DomainError::SiteNotFound(id.to_string())) }
    }
}

impl JsonSiteRepository {
    /// `update_sites` writes the file while holding the sites lock; run it on a
    /// blocking thread so neither the disk write nor a waiter on that lock
    /// stalls a Tokio worker.
    async fn update_off_runtime(
        &self,
        change: impl FnOnce(&mut std::collections::HashMap<String, LegacySite>) -> bool + Send + 'static,
    ) -> Result<bool, DomainError> {
        let state = self.state.clone();
        tokio::task::spawn_blocking(move || state.update_sites(change))
            .await
            .map_err(|e| DomainError::Persistence(format!("la tarea de guardado falló: {e}")))?
            .map_err(|e| write_failed(&e))
    }
}
