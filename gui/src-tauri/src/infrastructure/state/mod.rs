use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::infrastructure::{
    config::Config,
    dto::Site,
    fs,
    nginx::process::NginxProcess,
    persistence::site_mapper,
    php::process::PhpProcesses,
};

pub struct AppState {
    pub config:      RwLock<Config>,
    pub base_dir:    PathBuf,
    pub sites:       RwLock<HashMap<String, Site>>,
    pub nginx_proc:  Arc<NginxProcess>,
    pub php_proc:    Arc<PhpProcesses>,
    pub started_at:  Instant,
    /// Problems found while loading `sites.json` (quarantined file, records that
    /// fail validation). `AppContainer::new` forwards them to the logger.
    pub load_warnings: Vec<String>,
}

impl AppState {
    pub fn new(base_dir: PathBuf, config: Config) -> Arc<Self> {
        let certs_dir = config.certs_dir.clone();
        let fs::Loaded { value: sites, warning } = load_sites(&base_dir);
        let load_warnings = warning.into_iter()
            .chain(invalid_site_warnings(&sites, &certs_dir))
            .collect();
        Arc::new(Self {
            config:       RwLock::new(config),
            base_dir,
            sites:        RwLock::new(sites),
            nginx_proc:   NginxProcess::new(),
            php_proc:     PhpProcesses::new(),
            started_at:   Instant::now(),
            load_warnings,
        })
    }

    /// Applies `change` to a copy of the site map, writes it to `sites.json`,
    /// and only then publishes it — a failed write leaves memory and disk in
    /// agreement. Returns `Ok(false)` without writing when `change` reports
    /// that it changed nothing.
    ///
    /// The write lock is held across the file write so concurrent saves cannot
    /// interleave and drop each other's changes.
    pub fn update_sites(
        &self,
        change: impl FnOnce(&mut HashMap<String, Site>) -> bool,
    ) -> anyhow::Result<bool> {
        let mut sites = self.sites.write();
        let mut next = sites.clone();
        if !change(&mut next) { return Ok(false); }
        write_sites_file(&self.base_dir, &next)?;
        *sites = next;
        Ok(true)
    }
}

pub fn load_sites(base_dir: &Path) -> fs::Loaded<HashMap<String, Site>> {
    let fs::Loaded { value, warning } =
        fs::load_json_or(&base_dir.join("sites.json"), Vec::<Site>::new);
    fs::Loaded {
        value: value.into_iter().map(|s| (s.id.clone(), s)).collect(),
        warning,
    }
}

/// Records that cannot become a domain `Site` are kept on disk untouched but
/// hidden from the API; report them once at startup instead of on every read.
fn invalid_site_warnings<'a>(
    sites: &'a HashMap<String, Site>,
    certs_dir: &'a str,
) -> impl Iterator<Item = String> + 'a {
    sites.values().filter_map(move |s| {
        site_mapper::to_domain(s, Some(certs_dir)).err().map(|e| format!(
            "sites.json: ignoring site {} ({}): {e} — fix or remove it by hand",
            s.id, s.domain,
        ))
    })
}

fn write_sites_file(base_dir: &Path, sites: &HashMap<String, Site>) -> anyhow::Result<()> {
    let mut sorted: Vec<&Site> = sites.values().collect();
    sorted.sort_by(|a, b| a.domain.cmp(&b.domain));
    let data = serde_json::to_vec_pretty(&sorted)?;
    fs::atomic_write(&base_dir.join("sites.json"), &data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn config_for(base: &Path) -> Config {
        let mut cfg = Config::load(base).value;
        cfg.certs_dir = base.join("certs").to_string_lossy().into_owned();
        cfg
    }

    fn legacy_site(id: &str, domain: &str, path: &str) -> Site {
        Site {
            id: id.into(), name: domain.into(), domain: domain.into(), path: path.into(),
            php_version: "8.2".into(), project_type: "generic".into(), ssl_enabled: false,
            active: true, created_at: String::new(), updated_at: String::new(),
        }
    }

    fn valid_path() -> &'static str {
        if cfg!(windows) { r"C:\projects\app" } else { "/projects/app" }
    }

    #[test]
    fn corrupt_sites_json_is_preserved_and_reported() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("sites.json"), "{ not json").unwrap();

        let state = AppState::new(tmp.path().to_path_buf(), config_for(tmp.path()));
        assert!(state.sites.read().is_empty());
        assert_eq!(state.load_warnings.len(), 1);

        // A later save must not destroy the user's original data.
        state.update_sites(|m| {
            m.insert("a".into(), legacy_site("a", "a.test", valid_path()));
            true
        }).unwrap();
        let preserved = std::fs::read_dir(tmp.path()).unwrap().flatten()
            .any(|e| std::fs::read_to_string(e.path()).is_ok_and(|c| c == "{ not json"));
        assert!(preserved, "original corrupt content must survive in a .corrupt-* file");
    }

    #[test]
    fn invalid_records_are_reported_but_kept_on_disk() {
        let tmp = TempDir::new().unwrap();
        let sites = vec![
            legacy_site("good", "good.test", valid_path()),
            legacy_site("bad", "bad.com", valid_path()),
        ];
        std::fs::write(tmp.path().join("sites.json"), serde_json::to_vec(&sites).unwrap()).unwrap();

        let state = AppState::new(tmp.path().to_path_buf(), config_for(tmp.path()));
        assert_eq!(state.load_warnings.len(), 1);
        assert!(state.load_warnings[0].contains("bad.com"));

        state.update_sites(|m| {
            m.insert("new".into(), legacy_site("new", "new.test", valid_path()));
            true
        }).unwrap();
        let on_disk = std::fs::read_to_string(tmp.path().join("sites.json")).unwrap();
        assert!(on_disk.contains("bad.com"), "invalid records must not be dropped on save");
    }

    #[test]
    fn failed_write_leaves_cache_unchanged() {
        let tmp = TempDir::new().unwrap();
        // base_dir that does not exist → writing sites.json fails.
        let base = tmp.path().join("missing");
        let state = AppState::new(base.clone(), config_for(&base));

        let result = state.update_sites(|m| {
            m.insert("a".into(), legacy_site("a", "a.test", valid_path()));
            true
        });

        assert!(result.is_err());
        assert!(state.sites.read().is_empty(), "cache must not diverge from disk");
    }

    #[test]
    fn unchanged_update_does_not_write() {
        let tmp = TempDir::new().unwrap();
        let state = AppState::new(tmp.path().to_path_buf(), config_for(tmp.path()));
        assert!(!state.update_sites(|_| false).unwrap());
        assert!(!tmp.path().join("sites.json").exists());
    }
}
