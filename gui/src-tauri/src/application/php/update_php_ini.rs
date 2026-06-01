use std::{path::PathBuf, sync::Arc};
use crate::{
    infrastructure::dto::{PhpExtension, PhpSetting},
    domain::{
        errors::ApplicationError,
        ports::{process_manager::{PhpDetectorPort, PhpProcessPort}, web_server::WebServerPort},
    },
};
use super::ini_parser;

pub struct UpdatePhpIniCommand {
    pub major:              String,
    pub php_dir:            String,
    pub extension_changes:  Vec<PhpExtension>,
    pub setting_changes:    Vec<PhpSetting>,
}

pub struct UpdatePhpIniUseCase {
    php_proc:   Arc<dyn PhpProcessPort>,
    detector:   Arc<dyn PhpDetectorPort>,
    web_server: Arc<dyn WebServerPort>,
}

impl UpdatePhpIniUseCase {
    pub fn new(
        php_proc:   Arc<dyn PhpProcessPort>,
        detector:   Arc<dyn PhpDetectorPort>,
        web_server: Arc<dyn WebServerPort>,
    ) -> Self {
        Self { php_proc, detector, web_server }
    }

    pub async fn execute(&self, cmd: UpdatePhpIniCommand) -> Result<(), ApplicationError> {
        // Validar antes de tocar el archivo
        for ext in &cmd.extension_changes {
            ini_parser::validate_extension_name(&ext.name)
                .map_err(|e| ApplicationError::Infrastructure(
                    crate::domain::errors::InfrastructureError::ProcessFailed(e)
                ))?;
        }
        for s in &cmd.setting_changes {
            ini_parser::validate_setting(&s.key, &s.value)
                .map_err(|e| ApplicationError::Infrastructure(
                    crate::domain::errors::InfrastructureError::ProcessFailed(e)
                ))?;
        }

        let ini_path = PathBuf::from(&cmd.php_dir).join(&cmd.major).join("php.ini");
        let content = std::fs::read_to_string(&ini_path)
            .map_err(crate::domain::errors::InfrastructureError::Io)?;

        let after_ext   = ini_parser::apply_extension_changes(&content, &cmd.extension_changes);
        let new_content = ini_parser::apply_setting_changes(&after_ext, &cmd.setting_changes);

        std::fs::write(&ini_path, &new_content)
            .map_err(crate::domain::errors::InfrastructureError::Io)?;

        // Secuencia: stop PHP → reload nginx (limpia conexiones FastCGI) → restart PHP
        let was_running = self.php_proc.is_running(&cmd.major).await;
        if was_running {
            let _ = self.php_proc.stop(&cmd.major).await;
        }
        let _ = self.web_server.reload().await;

        if was_running {
            let versions = self.detector.detect().await;
            if let Some(v) = versions.iter().find(|v| v.major == cmd.major) {
                let _ = self.php_proc.start(v).await;
            }
        }

        Ok(())
    }
}
