use async_trait::async_trait;
use crate::domain::{errors::InfrastructureError, site::value_objects::PhpVersion};

/// Puerto de gestión de procesos PHP-CGI / PHP-FPM.
#[async_trait]
pub trait PhpProcessPort: Send + Sync {
    async fn start(&self, version: &PhpVersion) -> Result<(), InfrastructureError>;
    async fn stop(&self, version: &PhpVersion) -> Result<(), InfrastructureError>;
    async fn stop_all(&self) -> Result<(), InfrastructureError>;
    async fn is_running(&self, version: &PhpVersion) -> bool;
    async fn running_versions(&self) -> Vec<PhpVersion>;
}
