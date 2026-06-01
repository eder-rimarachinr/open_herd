use async_trait::async_trait;
use std::path::{Path, PathBuf};
use crate::infrastructure::ssl::mkcert as legacy_ssl;
use crate::domain::{errors::InfrastructureError, ports::ssl::SslPort};

pub struct MkcertAdapter { base_dir: PathBuf }
impl MkcertAdapter {
    pub fn new(base_dir: PathBuf) -> Self { Self { base_dir } }
    fn mkcert_binary(&self) -> Result<PathBuf, InfrastructureError> {
        legacy_ssl::ensure_mkcert(&self.base_dir).map_err(|e| InfrastructureError::NotFound(format!("mkcert: {}", e)))
    }
}

#[async_trait]
impl SslPort for MkcertAdapter {
    async fn issue_certificate(&self, domain: &str, certs_dir: &Path) -> Result<(String, String), InfrastructureError> {
        let mkcert = self.mkcert_binary()?;
        legacy_ssl::install_ca(&mkcert).map_err(|e| InfrastructureError::ProcessFailed(e))?;
        let paths = legacy_ssl::issue_cert(&mkcert, domain, certs_dir).map_err(|e| InfrastructureError::ProcessFailed(e))?;
        Ok((paths.cert.to_string_lossy().into_owned(), paths.key.to_string_lossy().into_owned()))
    }
    async fn revoke_certificate(&self, domain: &str) -> Result<(), InfrastructureError> {
        legacy_ssl::revoke_cert(domain, &self.base_dir.join("certs")); Ok(())
    }
}
