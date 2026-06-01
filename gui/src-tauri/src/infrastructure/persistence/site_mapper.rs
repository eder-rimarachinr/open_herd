/// Conversiones bidireccionales entre `daemon::models::Site` (modelo legacy, serde/JSON)
/// y `domain::site::entity::Site` (entidad de dominio, con value objects).
///
/// Esta capa de traducción es la única que conoce los dos mundos a la vez.
/// Cuando el dominio evolucione (p.ej. SslStatus con más variantes), solo
/// cambia este archivo — los adaptadores y los casos de uso no se tocan.
use crate::daemon::models::Site as LegacySite;
use crate::domain::site::{
    entity::{ProjectType, Site, SslStatus},
    value_objects::{DomainName, PhpVersion, SiteId, SitePath},
};

/// Convierte el modelo legacy al aggregate de dominio.
/// Se llama al leer desde `AppState` o desde sites.json.
///
/// Campos que no tienen representación directa en el legacy:
/// - `SslStatus::Active { cert_path, key_path }`: el legacy solo guarda `ssl_enabled: bool`.
///   Los paths se reconstruyen a partir de `certs_dir` que debe pasarse cuando ssl_enabled=true.
pub fn to_domain(legacy: &LegacySite, certs_dir: Option<&str>) -> Site {
    let domain = DomainName::new(&legacy.domain)
        .unwrap_or_else(|_| DomainName::new("invalid.test").expect("fallback domain"));

    let path = SitePath::new(&legacy.path)
        .unwrap_or_else(|_| {
            // En Windows podría ser que la ruta sea relativa en datos legacy —
            // usamos la raíz como fallback seguro hasta que el usuario la corrija.
            #[cfg(target_os = "windows")]
            return SitePath::new(r"C:\").expect("fallback path");
            #[cfg(not(target_os = "windows"))]
            return SitePath::new("/").expect("fallback path");
        });

    let php_version = PhpVersion::parse(&legacy.php_version);

    let ssl = if legacy.ssl_enabled {
        if let Some(certs) = certs_dir {
            SslStatus::Active {
                cert_path: format!("{}/{}.pem", certs, legacy.domain),
                key_path:  format!("{}/{}-key.pem", certs, legacy.domain),
            }
        } else {
            // Sin certs_dir no podemos reconstruir los paths; marcamos como activo con paths vacíos.
            SslStatus::Active { cert_path: String::new(), key_path: String::new() }
        }
    } else {
        SslStatus::Disabled
    };

    let project_type: ProjectType = legacy.project_type.parse().unwrap_or(ProjectType::Generic);

    let created_at = chrono::DateTime::parse_from_rfc3339(&legacy.created_at)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());

    let updated_at = chrono::DateTime::parse_from_rfc3339(&legacy.updated_at)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now());

    Site {
        id: SiteId::from_string(legacy.id.clone()),
        name: legacy.name.clone(),
        domain,
        path,
        php_version,
        ssl,
        project_type,
        active: legacy.active,
        created_at,
        updated_at,
    }
}

/// Convierte la entidad de dominio al modelo legacy para persistencia JSON.
pub fn to_legacy(site: &Site) -> LegacySite {
    LegacySite {
        id:           site.id.to_string(),
        name:         site.name.clone(),
        domain:       site.domain.to_string(),
        path:         site.path.to_string(),
        php_version:  site.php_version.as_ref().map(|v| v.display()).unwrap_or_default(),
        project_type: site.project_type.to_string(),
        ssl_enabled:  site.ssl.is_enabled(),
        active:       site.active,
        created_at:   site.created_at.to_rfc3339(),
        updated_at:   site.updated_at.to_rfc3339(),
    }
}
