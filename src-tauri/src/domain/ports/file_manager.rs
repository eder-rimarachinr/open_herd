use async_trait::async_trait;
use std::path::Path;
use crate::domain::errors::InfrastructureError;

/// Puerto para mostrar una carpeta al usuario en el explorador de archivos del
/// sistema. Es un efecto sobre el escritorio del usuario: los tests usan un doble.
#[async_trait]
pub trait FileManagerPort: Send + Sync {
    async fn open_folder(&self, path: &Path) -> Result<(), InfrastructureError>;
}
