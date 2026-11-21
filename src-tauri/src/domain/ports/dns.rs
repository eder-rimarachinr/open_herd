use async_trait::async_trait;
use crate::domain::errors::InfrastructureError;

/// Puerto de gestión de DNS local.
/// En Windows lo implementa HostsAdapter (/etc/hosts);
/// en Linux podría ser dnsmasq u otro mecanismo.
#[async_trait]
pub trait DnsPort: Send + Sync {
    /// Añade la entrada 127.0.0.1 → domain al resolver local. Idempotente.
    async fn add_entry(&self, domain: &str) -> Result<(), InfrastructureError>;
    /// Elimina la entrada del resolver local.
    async fn remove_entry(&self, domain: &str) -> Result<(), InfrastructureError>;
    /// Several entries in one change (scan / bulk add): the hosts adapter
    /// writes once, so a needed administrator prompt appears once.
    async fn add_entries(&self, domains: &[String]) -> Result<(), InfrastructureError> {
        for domain in domains {
            self.add_entry(domain).await?;
        }
        Ok(())
    }
}
