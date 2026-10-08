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

/// Shared by scan and bulk-add: vhost → DNS → persist for one new site.
/// Returns `false` (already logged) when the site was skipped because its
/// vhost or its persistence failed; a DNS failure is tolerated.
async fn register_new_site(
    site_repo: &dyn SiteRepository,
    web_server: &dyn WebServerPort,
    dns: &dyn DnsPort,
    site: &Site,
) -> bool {
    let domain = &site.domain;
    if !best_effort(web_server.create_vhost(site).await, format_args!("vhost error for {domain}")) {
        return false;
    }
    best_effort(dns.add_entry(domain.as_str()).await, format_args!("DNS error for {domain}"));
    best_effort(site_repo.save(site).await, format_args!("persist error for {domain}"))
}
