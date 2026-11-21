use std::{path::Path, sync::Arc};

use super::default_php;
use crate::domain::{
    errors::ApplicationError,
    ports::{composer::ComposerManagerPort, process_manager::PhpDetectorPort},
};

/// Points an installed `composer` command at the default PHP, after it
/// changes in Settings. Nothing to do when Composer is not installed or that
/// PHP is not (the status then explains why `composer` can't run).
pub struct SyncComposerCommandUseCase {
    manager: Arc<dyn ComposerManagerPort>,
    detector: Arc<dyn PhpDetectorPort>,
}

impl SyncComposerCommandUseCase {
    pub fn new(manager: Arc<dyn ComposerManagerPort>, detector: Arc<dyn PhpDetectorPort>) -> Self {
        Self { manager, detector }
    }

    /// `true` if the command was rewritten.
    pub async fn execute(&self, default_major: &str) -> Result<bool, ApplicationError> {
        let env = self.manager.environment().await?;
        if env.installed.is_none() { return Ok(false); }
        let Some(php) = default_php(self.detector.as_ref(), default_major).await else { return Ok(false) };
        if env.command_php.as_deref() == Some(Path::new(&php.binary_path)) { return Ok(false); }
        self.manager.write_command(Path::new(&php.binary_path)).await?;
        Ok(true)
    }
}
