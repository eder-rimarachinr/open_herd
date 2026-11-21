use std::{path::PathBuf, sync::Arc};

use super::default_php;
use crate::domain::{
    composer::{php_version_id, select_release},
    errors::{ApplicationError, DomainError},
    ports::{
        composer::{ComposerManagerPort, ComposerReleasesPort},
        download::{DownloadProgressPort, PostInstall},
        process_manager::PhpDetectorPort,
        task::TaskState,
    },
};

/// Installs or updates Composer to the newest release the default PHP can
/// run (2.2 LTS for PHP < 7.2.5), then writes the `composer` command. When
/// that release is already installed it only rewrites the command (Repair).
pub struct InstallComposerUseCase {
    manager: Arc<dyn ComposerManagerPort>,
    releases: Arc<dyn ComposerReleasesPort>,
    detector: Arc<dyn PhpDetectorPort>,
    downloads: Arc<dyn DownloadProgressPort>,
}

impl InstallComposerUseCase {
    pub fn new(
        manager: Arc<dyn ComposerManagerPort>,
        releases: Arc<dyn ComposerReleasesPort>,
        detector: Arc<dyn PhpDetectorPort>,
        downloads: Arc<dyn DownloadProgressPort>,
    ) -> Self {
        Self { manager, releases, detector, downloads }
    }

    /// The running download's state, `Done` after a repair, `Pending` once a
    /// new download has started.
    pub async fn execute(&self, default_major: &str) -> Result<TaskState, ApplicationError> {
        if let Some(p) = self.downloads.composer_progress().await {
            if p.state.is_active() { return Ok(p.state); }
        }
        let php = default_php(self.detector.as_ref(), default_major).await
            .ok_or_else(|| DomainError::ComposerNeedsPhp(default_major.to_string()))?;
        let php_id = php_version_id(&php.version).ok_or_else(|| DomainError::NoComposerForPhp(php.version.clone()))?;
        let releases = self.releases.releases().await?;
        let release = select_release(&releases, php_id)
            .ok_or_else(|| DomainError::NoComposerForPhp(php.version.clone()))?
            .clone();

        let env = self.manager.environment().await?;
        let php_binary = PathBuf::from(&php.binary_path);
        if env.installed.as_ref().and_then(|i| i.version.as_deref()) == Some(release.version.as_str()) {
            self.manager.write_command(&php_binary).await?;
            return Ok(TaskState::Done);
        }

        let manager = self.manager.clone();
        let after: PostInstall = Box::new(move || Box::pin(async move {
            manager.write_command(&php_binary).await.map_err(|e| e.to_string())
        }));
        let dest = env.phar_path.parent().map(PathBuf::from).unwrap_or_default();
        self.downloads.start_composer_download(&release, &dest, after).await;
        Ok(TaskState::Pending)
    }
}
