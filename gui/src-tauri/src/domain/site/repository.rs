use async_trait::async_trait;
use super::{entity::Site, value_objects::{DomainName, SiteId}};
use crate::domain::errors::DomainError;

/// Puerto de persistencia de sitios. El dominio define el contrato;
/// la infraestructura lo implementa (JSON, SQLite, etc.).
#[async_trait]
pub trait SiteRepository: Send + Sync {
    async fn find_by_id(&self, id: &SiteId) -> Result<Option<Site>, DomainError>;
    async fn find_by_domain(&self, domain: &DomainName) -> Result<Option<Site>, DomainError>;
    async fn list_all(&self) -> Result<Vec<Site>, DomainError>;
    async fn save(&self, site: &Site) -> Result<(), DomainError>;
    async fn delete(&self, id: &SiteId) -> Result<(), DomainError>;
}
