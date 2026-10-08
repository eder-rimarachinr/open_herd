use async_trait::async_trait;
use crate::infrastructure::dns::hosts;
use crate::domain::{errors::InfrastructureError, ports::dns::DnsPort};

pub struct HostsAdapter;
impl Default for HostsAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl HostsAdapter { pub fn new() -> Self { Self } }

#[async_trait]
impl DnsPort for HostsAdapter {
    async fn add_entry(&self, domain: &str) -> Result<(), InfrastructureError> {
        hosts::add_entry(domain).map_err(|e| InfrastructureError::Io(std::io::Error::other(e)))
    }
    async fn remove_entry(&self, domain: &str) -> Result<(), InfrastructureError> {
        hosts::remove_entry(domain).map_err(|e| InfrastructureError::Io(std::io::Error::other(e)))
    }
}
