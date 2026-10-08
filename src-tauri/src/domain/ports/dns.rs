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
}
