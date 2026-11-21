pub mod bulk_add_sites;
pub mod create_site;
pub mod delete_site;
pub mod disable_ssl;
pub mod enable_ssl;
pub mod project_type_detector;
pub mod refresh_site_config;
pub mod scan_sites;
pub mod update_site;

mod tests;

use crate::application::best_effort;
use crate::domain::{
    ports::{dns::DnsPort, web_server::WebServerPort},
    site::{entity::Site, repository::SiteRepository},
};

/// Shared by scan and bulk-add: vhost → persist for one new site. DNS comes
/// afterwards for all of them at once ([`add_dns_entries`]).
/// Returns `false` (already logged) when the site was skipped because its
/// vhost or its persistence failed.
async fn register_new_site(
    site_repo: &dyn SiteRepository,
    web_server: &dyn WebServerPort,
    site: &Site,
) -> bool {
    let domain = &site.domain;
    if !best_effort(web_server.create_vhost(site).await, format_args!("vhost error for {domain}")) {
        return false;
    }
    best_effort(site_repo.save(site).await, format_args!("persist error for {domain}"))
}

/// One hosts-file change for every site a scan / bulk add registered, so an
/// administrator prompt (when needed) appears once. A failure is tolerated.
async fn add_dns_entries(dns: &dyn DnsPort, domains: &[String]) {
    if domains.is_empty() { return; }
    best_effort(dns.add_entries(domains).await, format_args!("DNS error for {}", domains.join(", ")));
}
