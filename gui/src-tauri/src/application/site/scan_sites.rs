use crate::application::best_effort;
use std::sync::Arc;

use crate::domain::{
    errors::ApplicationError,
    ports::{dns::DnsPort, web_server::WebServerPort},
    site::{entity::Site, repository::SiteRepository, value_objects::{DomainName, SitePath}},
};
use super::{project_type_detector, register_new_site};

pub struct ScanSitesCommand {
    /// Directorios a escanear — leídos de `config.scanned_dirs` por el handler.
    pub dirs: Vec<String>,
    pub default_php: Option<String>,
}

pub struct ScanSitesResult {
    pub added: Vec<Site>,
    pub not_found: Vec<String>,
}

/// Escanea directorios buscando subdirectorios que aún no estén registrados como sitios.
/// Crea un sitio por cada subdirectorio nuevo encontrado (no-fatal por item).
pub struct ScanSitesUseCase {
    site_repo:  Arc<dyn SiteRepository>,
    web_server: Arc<dyn WebServerPort>,
    dns:        Arc<dyn DnsPort>,
}

impl ScanSitesUseCase {
    pub fn new(
        site_repo:  Arc<dyn SiteRepository>,
        web_server: Arc<dyn WebServerPort>,
        dns:        Arc<dyn DnsPort>,
    ) -> Self {
        Self { site_repo, web_server, dns }
    }

    pub async fn execute(&self, cmd: ScanSitesCommand) -> Result<ScanSitesResult, ApplicationError> {
        let existing = self.site_repo.list_all().await?;
        // Use a mutable set so we can add entries mid-loop (prevents duplicates when
        // the same folder name appears in more than one scanned_dir).
        let mut seen_domains: std::collections::HashSet<String> =
            existing.iter().map(|s| s.domain.to_string()).collect();

        let mut added: Vec<Site>   = vec![];
        let mut not_found: Vec<String> = vec![];

        for dir in &cmd.dirs {
            let base = std::path::Path::new(dir);
            if !base.exists() {
                not_found.push(dir.clone());
                continue;
            }
            let entries = match std::fs::read_dir(base) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() { continue; }

                let name = path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                if name.starts_with('.') { continue; }  // skip hidden

                // Lowercase to match DomainName's normalized storage.
                let domain_str = format!("{}.test", name.to_lowercase());
                if seen_domains.contains(&domain_str) { continue; }

                let path_str = path.to_string_lossy().to_string();

                // Validar — dominios con caracteres inválidos se saltan silenciosamente
                let domain = match DomainName::new(&domain_str) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let site_path = match SitePath::new(&path_str) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let mut site = crate::domain::site::entity::Site::new(
                    name.clone(), domain, site_path,
                );
                site.project_type = project_type_detector::detect(&path_str);
                if let Some(ref ver) = cmd.default_php {
                    if let Some(v) = crate::domain::site::value_objects::PhpVersion::parse(ver) {
                        site.php_version = Some(v);
                    }
                }

                // Crear vhost; si falla este sitio se salta pero continúa el scan
                if !register_new_site(self.site_repo.as_ref(), self.web_server.as_ref(), self.dns.as_ref(), &site).await {
                continue;
            }

                seen_domains.insert(domain_str);
                added.push(site);
            }
        }

        // Un solo reload al final para todos los sitios del scan
        if !added.is_empty() {
            best_effort(self.web_server.reload().await, format_args!("nginx reload error"));
        }

        Ok(ScanSitesResult { added, not_found })
    }
}
