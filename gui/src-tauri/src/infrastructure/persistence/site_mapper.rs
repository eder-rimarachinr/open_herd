use crate::infrastructure::dto::Site as LegacySite;
use crate::domain::{
    errors::DomainError,
    site::{
        entity::{ProjectType, Site, SslStatus},
        value_objects::{DomainName, PhpVersion, SiteId, SitePath},
    },
};

/// Fails when the stored domain or path no longer pass validation. Callers must
/// not substitute placeholder values: saving or deleting such a site would then
/// act on the wrong vhost / hosts entry.
pub fn to_domain(legacy: &LegacySite, certs_dir: Option<&str>) -> Result<Site, DomainError> {
    let domain = DomainName::new(&legacy.domain)?;
    let path   = SitePath::new(&legacy.path)?;
    let php_version   = PhpVersion::parse(&legacy.php_version);
    let ssl           = if legacy.ssl_enabled {
        if let Some(certs) = certs_dir { SslStatus::Active { cert_path: format!("{}/{}.pem", certs, legacy.domain), key_path: format!("{}/{}-key.pem", certs, legacy.domain) } }
        else { SslStatus::Active { cert_path: String::new(), key_path: String::new() } }
    } else { SslStatus::Disabled };
    let project_type: ProjectType = legacy.project_type.parse().unwrap_or(ProjectType::Generic);
    let created_at = chrono::DateTime::parse_from_rfc3339(&legacy.created_at).map(|dt| dt.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now());
    let updated_at = chrono::DateTime::parse_from_rfc3339(&legacy.updated_at).map(|dt| dt.with_timezone(&chrono::Utc)).unwrap_or_else(|_| chrono::Utc::now());
    Ok(Site { id: SiteId::from_string(legacy.id.clone()), name: legacy.name.clone(), domain, path, php_version, ssl, project_type, active: legacy.active, created_at, updated_at })
}

pub fn to_legacy(site: &Site) -> LegacySite {
    LegacySite {
        id: site.id.to_string(), name: site.name.clone(), domain: site.domain.to_string(),
        path: site.path.to_string(), php_version: site.php_version.as_ref().map(PhpVersion::display).unwrap_or_default(),
        project_type: site.project_type.to_string(), ssl_enabled: site.ssl.is_enabled(),
        active: site.active, created_at: site.created_at.to_rfc3339(), updated_at: site.updated_at.to_rfc3339(),
    }
}
