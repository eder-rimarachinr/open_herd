use async_trait::async_trait;
use std::path::Path;
use crate::domain::errors::InfrastructureError;

/// Puerto de gestión de certificados SSL locales.
/// Implementado por MkcertAdapter; intercambiable por Let's Encrypt u otro.
#[async_trait]
pub trait SslPort: Send + Sync {
    /// Emite un certificado para el dominio dado y devuelve (cert_path, key_path).
    async fn issue_certificate(
        &self,
        domain: &str,
        certs_dir: &Path,
    ) -> Result<(String, String), InfrastructureError>;

    /// Revoca (elimina) el certificado del dominio.
    async fn revoke_certificate(&self, domain: &str) -> Result<(), InfrastructureError>;
}
