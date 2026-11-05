use std::sync::Arc;

use crate::domain::{
    errors::{ApplicationError, DomainError},
    node::NodeVersion,
    ports::{download::DownloadProgressPort, node::NodeManagerPort},
};

/// Deletes an installed version that is neither active nor being downloaded.
pub struct UninstallNodeUseCase {
    manager: Arc<dyn NodeManagerPort>,
    downloads: Arc<dyn DownloadProgressPort>,
}

impl UninstallNodeUseCase {
    pub fn new(manager: Arc<dyn NodeManagerPort>, downloads: Arc<dyn DownloadProgressPort>) -> Self {
        Self { manager, downloads }
    }

    pub async fn execute(&self, raw_version: &str) -> Result<(), ApplicationError> {
        let version = NodeVersion::parse(raw_version)?;
        if self.downloads.node_progress(&version).await.is_some_and(|p| p.state.is_active()) {
            return Err(DomainError::NodeVersionBusy(version.to_string()).into());
        }
        let env = self.manager.environment().await?;
        if env.active.as_ref() == Some(&version) {
            return Err(DomainError::NodeVersionActive(version.to_string()).into());
        }
        if !env.installed.iter().any(|i| i.version == version) {
            return Err(DomainError::NodeVersionNotInstalled(version.to_string()).into());
        }
        self.manager.uninstall(&version).await?;
        Ok(())
    }
}
