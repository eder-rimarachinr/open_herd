use std::{path::PathBuf, sync::Arc};
use crate::daemon::download::DownloadState;
use crate::domain::errors::ApplicationError;

pub struct DownloadNginxUseCase {
    downloads: Arc<DownloadState>,
}

impl DownloadNginxUseCase {
    pub fn new(downloads: Arc<DownloadState>) -> Self { Self { downloads } }

    pub async fn execute(&self, nginx_dir: &str) -> Result<String, ApplicationError> {
        {
            let current = self.downloads.nginx.lock();
            if let Some(ref p) = *current {
                if p.state == "downloading" || p.state == "extracting" {
                    return Ok(p.state.clone());
                }
            }
        }
        let dir = PathBuf::from(nginx_dir);
        crate::daemon::download::download_nginx(&dir, self.downloads.clone());
        Ok("pending".into())
    }
}
