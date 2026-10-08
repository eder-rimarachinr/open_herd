use std::sync::Arc;
use crate::domain::{
    errors::{ApplicationError, InfrastructureError},
    ports::process_manager::{PhpDetectorPort, PhpInstallation, PhpProcessPort, PhpVersionRepository},
};

pub struct StartPhpUseCase {
    detector: Arc<dyn PhpDetectorPort>,
    process:  Arc<dyn PhpProcessPort>,
    versions: Arc<dyn PhpVersionRepository>,
}

impl StartPhpUseCase {
    pub fn new(
        detector: Arc<dyn PhpDetectorPort>,
        process:  Arc<dyn PhpProcessPort>,
        versions: Arc<dyn PhpVersionRepository>,
    ) -> Self {
        Self { detector, process, versions }
    }

    /// Inicia PHP-CGI para el `major` dado (ej. "8.2"). Usa la caché de
    /// versiones detectadas; si no está ahí (caché vacía o PHP recién
    /// instalado), detecta de nuevo y actualiza la caché.
    pub async fn execute(&self, major: &str) -> Result<(), ApplicationError> {
        let installation = match find(self.versions.list().await, major) {
            Some(found) => found,
            None => {
                let fresh = self.detector.detect().await;
                self.versions.replace(fresh.clone()).await;
                find(fresh, major).ok_or_else(|| InfrastructureError::NotFound(
                    format!("PHP {} no está instalado", major),
                ))?
            }
        };

        self.process.start(&installation).await.map_err(ApplicationError::Infrastructure)
    }
}

fn find(versions: Vec<PhpInstallation>, major: &str) -> Option<PhpInstallation> {
    versions.into_iter().find(|v| v.major == major || v.version.starts_with(major))
}
