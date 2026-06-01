use std::sync::Arc;
use crate::domain::{
    errors::ApplicationError,
    ports::{
        process_manager::{PhpDetectorPort, PhpProcessPort},
        web_server::WebServerPort,
    },
};

pub struct StartServicesCommand {
    /// Versión major de PHP a iniciar, ej. "8.2".
    pub default_php: String,
}

/// Orquesta el arranque de todos los servicios: detecta PHP si es necesario,
/// inicia PHP-CGI para la versión por defecto, y arranca nginx.
pub struct StartServicesUseCase {
    detector:  Arc<dyn PhpDetectorPort>,
    php_proc:  Arc<dyn PhpProcessPort>,
    web_server: Arc<dyn WebServerPort>,
}

impl StartServicesUseCase {
    pub fn new(
        detector:   Arc<dyn PhpDetectorPort>,
        php_proc:   Arc<dyn PhpProcessPort>,
        web_server: Arc<dyn WebServerPort>,
    ) -> Self {
        Self { detector, php_proc, web_server }
    }

    pub async fn execute(&self, cmd: StartServicesCommand) -> Result<(), ApplicationError> {
        // 1. Detectar PHP instalado
        let versions = self.detector.detect().await;

        // 2. Iniciar la versión por defecto (non-fatal si no existe)
        if let Some(v) = versions.iter().find(|v| {
            v.major == cmd.default_php || v.version.starts_with(&cmd.default_php)
        }) {
            if let Err(e) = self.php_proc.start(v).await {
                eprintln!("[services] PHP start warning: {}", e);
            }
        }

        // 3. Iniciar nginx (non-fatal — puede que ya esté corriendo)
        if let Err(e) = self.web_server.start().await {
            eprintln!("[services] Nginx start warning: {}", e);
        }

        Ok(())
    }
}
