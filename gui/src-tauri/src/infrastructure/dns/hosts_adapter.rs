use async_trait::async_trait;
use crate::infrastructure::{blocking, dns::hosts};
use crate::domain::{errors::InfrastructureError, ports::dns::DnsPort};

#[derive(Default)]
pub struct HostsAdapter;
impl HostsAdapter { pub fn new() -> Self { Self } }

fn to_infra(e: String) -> InfrastructureError { InfrastructureError::Io(std::io::Error::other(e)) }

#[async_trait]
impl DnsPort for HostsAdapter {
    async fn add_entry(&self, domain: &str) -> Result<(), InfrastructureError> {
        let domain = domain.to_owned();
        blocking::run(move || hosts::add_entry(&domain).map_err(to_infra)).await
    }
    async fn remove_entry(&self, domain: &str) -> Result<(), InfrastructureError> {
        let domain = domain.to_owned();
        blocking::run(move || hosts::remove_entry(&domain).map_err(to_infra)).await
    }
}
