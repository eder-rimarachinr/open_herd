use async_trait::async_trait;

use crate::daemon::dns;
use crate::domain::{errors::InfrastructureError, ports::dns::DnsPort};

/// Implementa `DnsPort` modificando el archivo hosts del sistema operativo.
/// En Windows: C:\Windows\System32\drivers\etc\hosts
/// En Linux/macOS: /etc/hosts
pub struct HostsAdapter;

impl HostsAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl DnsPort for HostsAdapter {
    async fn add_entry(&self, domain: &str) -> Result<(), InfrastructureError> {
        dns::add_entry(domain)
            .map_err(|e| InfrastructureError::Io(std::io::Error::other(e)))
    }

    async fn remove_entry(&self, domain: &str) -> Result<(), InfrastructureError> {
        dns::remove_entry(domain)
            .map_err(|e| InfrastructureError::Io(std::io::Error::other(e)))
    }
}
