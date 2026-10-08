use async_trait::async_trait;
use std::path::{Path, PathBuf};
use crate::infrastructure::ssl::mkcert as legacy_ssl;
use crate::domain::{errors::InfrastructureError, ports::ssl::SslPort};

pub struct MkcertAdapter { base_dir: PathBuf }
impl MkcertAdapter {
    pub fn new(base_dir: PathBuf) -> Self { Self { base_dir } }
}

#[async_trait]
impl SslPort for MkcertAdapter {
    async fn issue_certificate(&self, domain: &str, certs_dir: &Path) -> Result<(String, String), InfrastructureError> {
        // Download (async) first, then run the mkcert subprocesses on the
        // blocking pool.
        let mkcert = legacy_ssl::ensure_mkcert(&self.base_dir).await
            .map_err(|e| InfrastructureError::NotFound(format!("mkcert: {}", e)))?;
        let domain    = domain.to_string();
        let certs_dir = certs_dir.to_path_buf();
        crate::infrastructure::blocking::run(move || {
            legacy_ssl::install_ca(&mkcert).map_err(InfrastructureError::ProcessFailed)?;
            let paths = legacy_ssl::issue_cert(&mkcert, &domain, &certs_dir)
                .map_err(InfrastructureError::ProcessFailed)?;
            Ok((paths.cert.to_string_lossy().into_owned(), paths.key.to_string_lossy().into_owned()))
        }).await
    }
    async fn revoke_certificate(&self, domain: &str, certs_dir: &Path) -> Result<(), InfrastructureError> {
        let (domain, certs_dir) = (domain.to_owned(), certs_dir.to_path_buf());
        crate::infrastructure::blocking::run(move || { legacy_ssl::revoke_cert(&domain, &certs_dir); Ok(()) }).await
    }
}
