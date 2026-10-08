use std::{path::PathBuf, sync::Arc};
use crate::domain::{errors::ApplicationError, ports::{download::DownloadProgressPort, task::TaskState}};

pub struct InstallPhpUseCase {
    downloads: Arc<dyn DownloadProgressPort>,
}

impl InstallPhpUseCase {
    pub fn new(downloads: Arc<dyn DownloadProgressPort>) -> Self { Self { downloads } }

    /// Starts the download unless one is already in flight for `major`.
    pub async fn execute(&self, major: &str, php_dir: &str) -> Result<TaskState, ApplicationError> {
        if let Some(prog) = self.downloads.php_progress(major).await {
            if prog.state.is_active() { return Ok(prog.state); }
        }
        let dir = PathBuf::from(php_dir);
        self.downloads.start_php_download(major, &dir).await;
        Ok(TaskState::Pending)
    }
}
