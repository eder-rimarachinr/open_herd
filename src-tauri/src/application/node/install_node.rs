use std::sync::Arc;

use crate::domain::{
    errors::ApplicationError,
    node::NodeVersion,
    ports::{download::DownloadProgressPort, node::NodeManagerPort, task::TaskState},
};

/// Downloads a Node version into the manager's store (nvm's folder when nvm
/// is installed, so `nvm list` sees it too).
pub struct InstallNodeUseCase {
    manager: Arc<dyn NodeManagerPort>,
    downloads: Arc<dyn DownloadProgressPort>,
}

impl InstallNodeUseCase {
    pub fn new(manager: Arc<dyn NodeManagerPort>, downloads: Arc<dyn DownloadProgressPort>) -> Self {
        Self { manager, downloads }
    }

    /// `Done` if already installed, the current state if a download for it is
    /// in flight, `Pending` once a new download has started.
    pub async fn execute(&self, raw_version: &str) -> Result<TaskState, ApplicationError> {
        let version = NodeVersion::parse(raw_version)?;
        if let Some(p) = self.downloads.node_progress(&version).await {
            if p.state.is_active() { return Ok(p.state); }
        }
        let env = self.manager.environment().await?;
        if env.installed.iter().any(|i| i.version == version) { return Ok(TaskState::Done); }
        self.downloads.start_node_download(&version, &env.store_dir.join(version.dir_name())).await;
        Ok(TaskState::Pending)
    }
}
