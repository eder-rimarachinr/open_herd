use std::{path::PathBuf, sync::Arc};
use crate::infrastructure::download::DownloadState;
use crate::domain::errors::ApplicationError;

pub struct InstallPhpUseCase {
    downloads: Arc<DownloadState>,
}

impl InstallPhpUseCase {
    pub fn new(downloads: Arc<DownloadState>) -> Self { Self { downloads } }

    pub async fn execute(&self, major: &str, php_dir: &str) -> Result<String, ApplicationError> {
        // No iniciar si ya hay una descarga en curso
        if let Some(prog) = self.downloads.php.lock().get(major) {
            if prog.state == "downloading" || prog.state == "extracting" {
                return Ok(prog.state.clone());
            }
        }
        let dir = PathBuf::from(php_dir);
        crate::infrastructure::download::download_php(major, &dir, self.downloads.clone());
        Ok("pending".into())
    }
}
