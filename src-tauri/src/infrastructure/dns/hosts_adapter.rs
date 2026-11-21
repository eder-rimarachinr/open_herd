use async_trait::async_trait;
use crate::infrastructure::{blocking, dns::hosts::{self, HostsEdit, HostsError}};
use crate::domain::{errors::InfrastructureError, ports::dns::DnsPort};

#[derive(Default)]
pub struct HostsAdapter;
impl HostsAdapter { pub fn new() -> Self { Self } }

fn to_infra(e: impl std::fmt::Display) -> InfrastructureError {
    InfrastructureError::Io(std::io::Error::other(e.to_string()))
}

/// Tries a direct write; when only an administrator may write the file,
/// Windows gets the elevated helper (one UAC prompt) on a thread of its own,
/// so the request that triggered it returns at once and the GUI's timeout
/// never races the user answering the prompt. Its outcome goes to the log.
async fn apply(edit: HostsEdit) -> Result<(), InfrastructureError> {
    blocking::run(move || match hosts::edit(&edit) {
        Ok(()) => Ok(()),
        Err(HostsError::NeedsElevation) => elevate(edit),
        Err(e) => Err(to_infra(e)),
    }).await
}

#[cfg(windows)]
fn elevate(edit: HostsEdit) -> Result<(), InfrastructureError> {
    tracing::info!("Asking for administrator permission to update the hosts file ({})", edit.describe());
    std::thread::Builder::new()
        .name("open-herd-hosts-elevated".into())
        .spawn(move || match super::elevated::edit_elevated(&edit) {
            Ok(()) => tracing::info!("Hosts file updated: {}", edit.describe()),
            Err(e) => tracing::warn!("Hosts file not updated ({}): {e}", edit.describe()),
        })
        .map(drop)
        .map_err(to_infra)
}

#[cfg(not(windows))]
fn elevate(_edit: HostsEdit) -> Result<(), InfrastructureError> {
    Err(to_infra(HostsError::NeedsElevation))
}

#[async_trait]
impl DnsPort for HostsAdapter {
    async fn add_entry(&self, domain: &str) -> Result<(), InfrastructureError> {
        apply(HostsEdit::Add(vec![domain.to_owned()])).await
    }
    async fn remove_entry(&self, domain: &str) -> Result<(), InfrastructureError> {
        apply(HostsEdit::Remove(domain.to_owned())).await
    }
    async fn add_entries(&self, domains: &[String]) -> Result<(), InfrastructureError> {
        apply(HostsEdit::Add(domains.to_vec())).await
    }
}
