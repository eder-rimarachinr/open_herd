use std::{path::PathBuf, sync::Arc};
use crate::domain::{errors::ApplicationError, ports::download::DownloadProgressPort};

pub struct InstallPhpUseCase {
    downloads: Arc<dyn DownloadProgressPort>,
}

impl InstallPhpUseCase {
    pub fn new(downloads: Arc<dyn DownloadProgressPort>) -> Self { Self { downloads } }

    pub async fn execute(&self, major: &str, php_dir: &str) -> Result<String, ApplicationError> {
        if let Some(prog) = self.downloads.php_progress(major).await {
            if prog.state == "downloading" || prog.state == "extracting" {
                return Ok(prog.state);
            }
        }
        let dir = PathBuf::from(php_dir);
        self.downloads.start_php_download(major, &dir).await;
        Ok("pending".into())
    }
}
