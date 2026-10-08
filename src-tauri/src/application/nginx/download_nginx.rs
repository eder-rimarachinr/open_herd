use std::{path::PathBuf, sync::Arc};
use crate::domain::{errors::ApplicationError, ports::{download::DownloadProgressPort, task::TaskState}};

pub struct DownloadNginxUseCase {
    downloads: Arc<dyn DownloadProgressPort>,
}

impl DownloadNginxUseCase {
    pub fn new(downloads: Arc<dyn DownloadProgressPort>) -> Self { Self { downloads } }

    /// Starts the download unless one is already in flight.
    pub async fn execute(&self, nginx_dir: &str) -> Result<TaskState, ApplicationError> {
        if let Some(prog) = self.downloads.nginx_progress().await {
            if prog.state.is_active() { return Ok(prog.state); }
        }
        let dir = PathBuf::from(nginx_dir);
        self.downloads.start_nginx_download(&dir).await;
        Ok(TaskState::Pending)
    }
}
