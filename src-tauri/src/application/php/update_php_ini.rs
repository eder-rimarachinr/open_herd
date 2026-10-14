use std::sync::Arc;
use crate::application::best_effort;
use crate::domain::{
    errors::{ApplicationError, DomainError},
    ports::{
        php_ini::{PhpIniFile, PhpIniStore},
        process_manager::{PhpDetectorPort, PhpProcessPort},
        web_server::WebServerPort,
    },
};
use super::{
    get_php_ini::PhpIniView,
    ini_parser::{self, ExtensionChange, SettingChange},
};

pub struct UpdatePhpIniCommand {
    pub major:             String,
    pub extension_changes: Vec<ExtensionChange>,
    pub setting_changes:   Vec<SettingChange>,
}

pub struct UpdatePhpIniUseCase {
    store:      Arc<dyn PhpIniStore>,
    php_proc:   Arc<dyn PhpProcessPort>,
    detector:   Arc<dyn PhpDetectorPort>,
    web_server: Arc<dyn WebServerPort>,
}

impl UpdatePhpIniUseCase {
    pub fn new(
        store:      Arc<dyn PhpIniStore>,
        php_proc:   Arc<dyn PhpProcessPort>,
        detector:   Arc<dyn PhpDetectorPort>,
        web_server: Arc<dyn WebServerPort>,
    ) -> Self {
        Self { store, php_proc, detector, web_server }
    }

    /// Validates every change before touching the file, writes it, restarts
    /// that PHP version if it was running, and returns the new state.
    pub async fn execute(&self, cmd: UpdatePhpIniCommand) -> Result<PhpIniView, ApplicationError> {
        for ext in &cmd.extension_changes {
            ini_parser::validate_extension_name(&ext.name).map_err(DomainError::InvalidPhpIniChange)?;
        }
        for s in &cmd.setting_changes {
            ini_parser::validate_setting(&s.key, &s.value).map_err(DomainError::InvalidPhpIniChange)?;
        }

        let file = self.store.read(&cmd.major).await?
            .ok_or_else(|| DomainError::PhpIniNotFound(cmd.major.clone()))?;
        let after_ext = ini_parser::apply_extension_changes(&file.content, &cmd.extension_changes);
        let content   = ini_parser::apply_setting_changes(&after_ext, &cmd.setting_changes);
        self.store.write(&cmd.major, &content).await?;

        self.restart_if_running(&cmd.major).await;
        Ok(PhpIniView::of(&cmd.major, &PhpIniFile { path: file.path, content }))
    }

    /// php-cgi only reads php.ini at start: stop it, reload nginx (drops its
    /// FastCGI connections), start it again. Failures are logged, not fatal:
    /// the file is already saved.
    async fn restart_if_running(&self, major: &str) {
        if !self.php_proc.is_running(major).await { return; }
        best_effort(self.php_proc.stop(major).await, format_args!("PHP {major} stop after php.ini change"));
        best_effort(self.web_server.reload().await, format_args!("nginx reload after php.ini change"));
        match self.detector.detect().await.into_iter().find(|v| v.major == major) {
            Some(v) => { best_effort(self.php_proc.start(&v).await, format_args!("PHP {major} restart after php.ini change")); }
            None => tracing::warn!("PHP {major} was stopped to apply php.ini but is no longer detected; start it again by hand"),
        }
    }
}
