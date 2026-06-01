use std::sync::Arc;
use crate::domain::{
    errors::ApplicationError,
    ports::process_manager::{PhpDetectorPort, PhpInstallation, PhpProcessPort},
};

pub struct StartPhpUseCase {
    detector: Arc<dyn PhpDetectorPort>,
    process:  Arc<dyn PhpProcessPort>,
}

impl StartPhpUseCase {
    pub fn new(detector: Arc<dyn PhpDetectorPort>, process: Arc<dyn PhpProcessPort>) -> Self {
        Self { detector, process }
    }

    /// Inicia PHP-CGI para el `major` dado (ej. "8.2").
    /// Si la versión no está en el caché, realiza una detección automática.
    pub async fn execute(
        &self,
        major: &str,
        known: Option<Vec<PhpInstallation>>,
    ) -> Result<(), ApplicationError> {
        let versions = match known {
            Some(v) => v,
            None    => self.detector.detect().await,
        };

        let installation = versions
            .into_iter()
            .find(|v| v.major == major || v.version.starts_with(major))
            .ok_or_else(|| crate::domain::errors::DomainError::SiteNotFound(
                format!("PHP {} no detectado — ejecuta detect primero", major)
            ))?;

        self.process.start(&installation).await.map_err(ApplicationError::Infrastructure)
    }
}
