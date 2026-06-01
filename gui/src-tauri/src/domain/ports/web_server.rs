use async_trait::async_trait;
use crate::domain::{errors::InfrastructureError, site::entity::Site};

/// Puerto de servidor web. Hoy lo implementa NginxAdapter; mañana podría
/// ser Caddy, Apache, o un stub para tests.
#[async_trait]
pub trait WebServerPort: Send + Sync {
    /// Crea o regenera el vhost para el sitio.
    async fn create_vhost(&self, site: &Site) -> Result<(), InfrastructureError>;
    /// Elimina el vhost del sitio.
    async fn remove_vhost(&self, site: &Site) -> Result<(), InfrastructureError>;
    /// Recarga la configuración sin reiniciar el proceso.
    async fn reload(&self) -> Result<(), InfrastructureError>;
    async fn is_running(&self) -> bool;

    /// Inicia el proceso del servidor web. Implementación por defecto no-op
    /// para adaptadores que no gestionan el ciclo de vida del proceso.
    async fn start(&self) -> Result<(), InfrastructureError> { Ok(()) }
    /// Detiene el proceso del servidor web.
    async fn stop(&self) -> Result<(), InfrastructureError> { Ok(()) }
}
