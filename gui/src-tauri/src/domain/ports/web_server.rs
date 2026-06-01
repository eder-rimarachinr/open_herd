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
}
