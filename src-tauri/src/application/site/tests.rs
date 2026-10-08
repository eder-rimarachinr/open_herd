#![cfg(test)]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use async_trait::async_trait;

use crate::domain::{
    errors::{DomainError, InfrastructureError},
    ports::{dns::DnsPort, ssl::SslPort, web_server::WebServerPort},
    site::{
        entity::Site,
        repository::SiteRepository,
        value_objects::{DomainName, SiteId},
    },
};

use super::{
    create_site::{CreateSiteCommand, CreateSiteUseCase},
    delete_site::DeleteSiteUseCase,
    disable_ssl::DisableSslUseCase,
    enable_ssl::EnableSslUseCase,
};

// ── Mocks ─────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct MockSiteRepo {
    sites: Mutex<HashMap<String, Site>>,
}

#[async_trait]
impl SiteRepository for MockSiteRepo {
    async fn find_by_id(&self, id: &SiteId) -> Result<Option<Site>, DomainError> {
        Ok(self.sites.lock().unwrap().get(id.as_str()).cloned())
    }

    async fn find_by_domain(&self, domain: &DomainName) -> Result<Option<Site>, DomainError> {
        Ok(self
            .sites
            .lock()
            .unwrap()
            .values()
            .find(|s| s.domain.as_str() == domain.as_str())
            .cloned())
    }

    async fn list_all(&self) -> Result<Vec<Site>, DomainError> {
        Ok(self.sites.lock().unwrap().values().cloned().collect())
    }

    async fn save(&self, site: &Site) -> Result<(), DomainError> {
        self.sites.lock().unwrap().insert(site.id.to_string(), site.clone());
        Ok(())
    }

    async fn delete(&self, id: &SiteId) -> Result<(), DomainError> {
        self.sites.lock().unwrap().remove(id.as_str());
        Ok(())
    }
}

/// Registra todas las llamadas recibidas para poder hacer asserts sobre ellas.
#[derive(Default)]
struct SpyWebServer {
    vhosts_created: Mutex<Vec<String>>,
    vhosts_removed: Mutex<Vec<String>>,
    reload_count:   Mutex<u32>,
}

#[async_trait]
impl WebServerPort for SpyWebServer {
    async fn create_vhost(&self, site: &Site) -> Result<(), InfrastructureError> {
        self.vhosts_created.lock().unwrap().push(site.domain.to_string());
        Ok(())
    }

    async fn remove_vhost(&self, site: &Site) -> Result<(), InfrastructureError> {
        self.vhosts_removed.lock().unwrap().push(site.domain.to_string());
        Ok(())
    }

    async fn reload(&self) -> Result<(), InfrastructureError> {
        *self.reload_count.lock().unwrap() += 1;
        Ok(())
    }

    async fn is_running(&self) -> bool { true }
}

#[derive(Default)]
struct SpyDns {
    added:   Mutex<Vec<String>>,
    removed: Mutex<Vec<String>>,
}

#[async_trait]
impl DnsPort for SpyDns {
    async fn add_entry(&self, domain: &str) -> Result<(), InfrastructureError> {
        self.added.lock().unwrap().push(domain.to_string());
        Ok(())
    }

    async fn remove_entry(&self, domain: &str) -> Result<(), InfrastructureError> {
        self.removed.lock().unwrap().push(domain.to_string());
        Ok(())
    }
}

#[derive(Default)]
struct StubSsl;

#[async_trait]
impl SslPort for StubSsl {
    async fn issue_certificate(
        &self,
        domain: &str,
        _certs_dir: &Path,
    ) -> Result<(String, String), InfrastructureError> {
        Ok((
            format!("/certs/{}.pem", domain),
            format!("/certs/{}-key.pem", domain),
        ))
    }

    async fn revoke_certificate(&self, _domain: &str, _certs_dir: &Path) -> Result<(), InfrastructureError> {
        Ok(())
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn make_deps() -> (Arc<MockSiteRepo>, Arc<SpyWebServer>, Arc<SpyDns>) {
    (
        Arc::new(MockSiteRepo::default()),
        Arc::new(SpyWebServer::default()),
        Arc::new(SpyDns::default()),
    )
}

fn create_cmd(domain: &str) -> CreateSiteCommand {
    CreateSiteCommand {
        domain: domain.into(),
        #[cfg(target_os = "windows")]
        path: r"C:\dev\myapp".into(),
        #[cfg(not(target_os = "windows"))]
        path: "/home/dev/myapp".into(),
        default_php: Some("8.2".into()),
    }
}

// ── Tests: CreateSiteUseCase ──────────────────────────────────────────────────

#[tokio::test]
async fn create_site_persists_and_notifies_dns() {
    let (repo, web, dns) = make_deps();
    let uc = CreateSiteUseCase::new(repo.clone(), web.clone(), dns.clone());

    let site = uc.execute(create_cmd("myapp.test")).await.unwrap();

    // Sitio guardado en repo
    assert!(repo.find_by_id(&site.id).await.unwrap().is_some());
    // Vhost creado
    assert!(web.vhosts_created.lock().unwrap().contains(&"myapp.test".to_string()));
    // Nginx recargado
    assert_eq!(*web.reload_count.lock().unwrap(), 1);
    // DNS registrado
    assert!(dns.added.lock().unwrap().contains(&"myapp.test".to_string()));
}

#[tokio::test]
async fn create_site_rejects_duplicate_domain() {
    let (repo, web, dns) = make_deps();
    let uc = CreateSiteUseCase::new(repo.clone(), web.clone(), dns.clone());

    uc.execute(create_cmd("myapp.test")).await.unwrap();
    let result = uc.execute(create_cmd("myapp.test")).await;

    assert!(result.is_err());
}

#[tokio::test]
async fn create_site_rejects_invalid_domain() {
    let (repo, web, dns) = make_deps();
    let uc = CreateSiteUseCase::new(repo.clone(), web.clone(), dns.clone());

    let mut cmd = create_cmd("myapp.test");
    cmd.domain = "invalid.com".into();
    let result = uc.execute(cmd).await;

    assert!(result.is_err());
    // Nada persiste
    assert!(repo.list_all().await.unwrap().is_empty());
}

// ── Tests: DeleteSiteUseCase ──────────────────────────────────────────────────

#[tokio::test]
async fn delete_site_removes_vhost_and_dns() {
    let (repo, web, dns) = make_deps();
    let create_uc = CreateSiteUseCase::new(repo.clone(), web.clone(), dns.clone());
    let delete_uc = DeleteSiteUseCase::new(repo.clone(), web.clone(), dns.clone());

    let site = create_uc.execute(create_cmd("myapp.test")).await.unwrap();
    delete_uc.execute(site.id.as_str()).await.unwrap();

    // Ya no existe en el repo
    assert!(repo.find_by_id(&site.id).await.unwrap().is_none());
    // Vhost eliminado
    assert!(web.vhosts_removed.lock().unwrap().contains(&"myapp.test".to_string()));
    // DNS eliminado
    assert!(dns.removed.lock().unwrap().contains(&"myapp.test".to_string()));
}

#[tokio::test]
async fn delete_site_returns_error_if_not_found() {
    let (repo, web, dns) = make_deps();
    let uc = DeleteSiteUseCase::new(repo, web, dns);

    let result = uc.execute("nonexistent-id").await;
    assert!(result.is_err());
}

// ── Tests: EnableSslUseCase ───────────────────────────────────────────────────

#[tokio::test]
async fn enable_ssl_stores_cert_paths_on_site() {
    let (repo, web, dns) = make_deps();
    let create_uc = CreateSiteUseCase::new(repo.clone(), web.clone(), dns.clone());
    let site = create_uc.execute(create_cmd("myapp.test")).await.unwrap();

    let ssl_uc = EnableSslUseCase::new(
        repo.clone(),
        Arc::new(StubSsl),
        web.clone(),
        PathBuf::from("/certs"),
    );
    ssl_uc.execute(site.id.as_str()).await.unwrap();

    let updated = repo.find_by_id(&site.id).await.unwrap().unwrap();
    assert!(updated.ssl.is_enabled());
}

#[tokio::test]
async fn enable_ssl_twice_is_error() {
    let (repo, web, dns) = make_deps();
    let create_uc = CreateSiteUseCase::new(repo.clone(), web.clone(), dns.clone());
    let site = create_uc.execute(create_cmd("myapp.test")).await.unwrap();

    let ssl_uc = EnableSslUseCase::new(
        repo.clone(),
        Arc::new(StubSsl),
        web.clone(),
        PathBuf::from("/certs"),
    );
    ssl_uc.execute(site.id.as_str()).await.unwrap();
    let result = ssl_uc.execute(site.id.as_str()).await;
    assert!(result.is_err());
}

// ── Tests: DisableSslUseCase ──────────────────────────────────────────────────

#[tokio::test]
async fn disable_ssl_is_idempotent() {
    let (repo, web, dns) = make_deps();
    let create_uc = CreateSiteUseCase::new(repo.clone(), web.clone(), dns.clone());
    let site = create_uc.execute(create_cmd("myapp.test")).await.unwrap();

    let disable_uc = DisableSslUseCase::new(repo.clone(), Arc::new(StubSsl), web.clone(), PathBuf::from("/certs"));

    // Deshabilitar cuando ya está deshabilitado: no error
    disable_uc.execute(site.id.as_str()).await.unwrap();
    disable_uc.execute(site.id.as_str()).await.unwrap();
}

#[tokio::test]
async fn disable_ssl_after_enable_marks_disabled() {
    let (repo, web, dns) = make_deps();
    let create_uc = CreateSiteUseCase::new(repo.clone(), web.clone(), dns.clone());
    let site = create_uc.execute(create_cmd("myapp.test")).await.unwrap();

    let ssl_uc = EnableSslUseCase::new(
        repo.clone(),
        Arc::new(StubSsl),
        web.clone(),
        PathBuf::from("/certs"),
    );
    ssl_uc.execute(site.id.as_str()).await.unwrap();

    let disable_uc = DisableSslUseCase::new(repo.clone(), Arc::new(StubSsl), web.clone(), PathBuf::from("/certs"));
    disable_uc.execute(site.id.as_str()).await.unwrap();

    let updated = repo.find_by_id(&site.id).await.unwrap().unwrap();
    assert!(!updated.ssl.is_enabled());
}

/// Records where revocation was asked to delete certificates.
#[derive(Default)]
struct RecordingSsl { revoked_in: Mutex<Vec<PathBuf>> }

#[async_trait]
impl SslPort for RecordingSsl {
    async fn issue_certificate(&self, domain: &str, certs_dir: &Path) -> Result<(String, String), InfrastructureError> {
        StubSsl.issue_certificate(domain, certs_dir).await
    }
    async fn revoke_certificate(&self, _domain: &str, certs_dir: &Path) -> Result<(), InfrastructureError> {
        self.revoked_in.lock().unwrap().push(certs_dir.to_path_buf());
        Ok(())
    }
}

#[tokio::test]
async fn disable_ssl_revokes_in_the_configured_certs_dir() {
    let (repo, web, dns) = make_deps();
    let site = CreateSiteUseCase::new(repo.clone(), web.clone(), dns.clone())
        .execute(create_cmd("myapp.test")).await.unwrap();
    let certs = PathBuf::from("/custom/certs");
    let ssl = Arc::new(RecordingSsl::default());
    EnableSslUseCase::new(repo.clone(), ssl.clone(), web.clone(), certs.clone())
        .execute(site.id.as_str()).await.unwrap();

    DisableSslUseCase::new(repo.clone(), ssl.clone(), web.clone(), certs.clone())
        .execute(site.id.as_str()).await.unwrap();

    assert_eq!(*ssl.revoked_in.lock().unwrap(), vec![certs]);
}
