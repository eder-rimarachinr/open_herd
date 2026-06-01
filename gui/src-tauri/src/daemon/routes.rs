/// Funciones auxiliares del daemon que aún son usadas por `ports/http/`.
/// El resto de los handlers han sido migrados a la capa hexagonal.
use super::models::{CatalogEntry, PhpVersion};

/// Expuesto para que `php_handlers.rs` pueda construir el catálogo.
pub fn build_catalog_pub(versions: &[PhpVersion], running: &[String]) -> Vec<CatalogEntry> {
    build_catalog(versions, running)
}

fn build_catalog(versions: &[PhpVersion], running: &[String]) -> Vec<CatalogEntry> {
    let known: &[(&str, &str, bool, bool)] = &[
        ("8.5", "8.5.6",  false, false),
        ("8.4", "8.4.21", false, false),
        ("8.3", "8.3.31", false, false),
        ("8.2", "8.2.31", false, false),
        ("8.1", "8.1.34", true,  false),
        ("8.0", "8.0.30", false, true),
        ("7.4", "7.4.33", false, true),
    ];
    known.iter().map(|(major, latest, security_only, eol)| {
        let installed_ver = versions.iter().find(|v| v.major == *major);
        let is_running    = running.iter().any(|r| r == *major);
        let has_update    = installed_ver.map(|v| v.version != *latest).unwrap_or(false);
        CatalogEntry {
            major:           major.to_string(),
            latest_patch:    latest.to_string(),
            installed_patch: installed_ver.map(|v| v.version.clone()),
            installed:       installed_ver.is_some(),
            running:         is_running,
            has_update,
            security_only:   *security_only,
            end_of_life:     *eol,
        }
    }).collect()
}
