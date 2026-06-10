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
        // `ensure_mkcert` uses reqwest::blocking (which spins up and drops its own
        // runtime) and mkcert runs as a blocking subprocess. Run the whole sequence
        // on a blocking thread so we never create/drop a runtime inside the async
        // executor — that panics ("Cannot drop a runtime…") and strands the task.
        let base_dir  = self.base_dir.clone();
        let domain    = domain.to_string();
        let certs_dir = certs_dir.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let mkcert = legacy_ssl::ensure_mkcert(&base_dir)
                .map_err(|e| InfrastructureError::NotFound(format!("mkcert: {}", e)))?;
            legacy_ssl::install_ca(&mkcert).map_err(InfrastructureError::ProcessFailed)?;
            let paths = legacy_ssl::issue_cert(&mkcert, &domain, &certs_dir)
                .map_err(InfrastructureError::ProcessFailed)?;
            Ok((paths.cert.to_string_lossy().into_owned(), paths.key.to_string_lossy().into_owned()))
        })
        .await
        .map_err(|e| InfrastructureError::ProcessFailed(format!("SSL task panicked: {}", e)))?
    }
    async fn revoke_certificate(&self, domain: &str) -> Result<(), InfrastructureError> {
        legacy_ssl::revoke_cert(domain, &self.base_dir.join("certs")); Ok(())
    }
}
