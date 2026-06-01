use async_trait::async_trait;
use std::path::{Path, PathBuf};

use crate::daemon::ssl as legacy_ssl;
use crate::domain::{errors::InfrastructureError, ports::ssl::SslPort};

/// Implementa `SslPort` usando mkcert para emitir certificados locales de confianza.
/// Descarga mkcert automáticamente si no está presente e instala la CA raíz.
pub struct MkcertAdapter {
    base_dir: PathBuf,
}

impl MkcertAdapter {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn mkcert_binary(&self) -> Result<PathBuf, InfrastructureError> {
        legacy_ssl::ensure_mkcert(&self.base_dir)
            .map_err(|e| InfrastructureError::NotFound(format!("mkcert: {}", e)))
    }
}

#[async_trait]
impl SslPort for MkcertAdapter {
    async fn issue_certificate(
        &self,
        domain: &str,
        certs_dir: &Path,
    ) -> Result<(String, String), InfrastructureError> {
        let mkcert = self.mkcert_binary()?;

        // Instala la CA si aún no está en el trust store (idempotente).
        legacy_ssl::install_ca(&mkcert)
            .map_err(|e| InfrastructureError::ProcessFailed(e))?;

        let paths = legacy_ssl::issue_cert(&mkcert, domain, certs_dir)
            .map_err(|e| InfrastructureError::ProcessFailed(e))?;

        Ok((
            paths.cert.to_string_lossy().into_owned(),
            paths.key.to_string_lossy().into_owned(),
        ))
    }

    async fn revoke_certificate(&self, domain: &str) -> Result<(), InfrastructureError> {
        // Necesitamos el certs_dir; como revoke solo elimina archivos lo derivamos
        // del directorio base estándar. Si el archivo no existe, es no-op (silent).
        let certs_dir = self.base_dir.join("certs");
        legacy_ssl::revoke_cert(domain, &certs_dir);
        Ok(())
    }
}
