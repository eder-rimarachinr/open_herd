use std::{path::PathBuf, sync::Arc};
use crate::domain::{errors::ApplicationError, ports::download::DownloadProgressPort};

pub struct DownloadNginxUseCase {
    downloads: Arc<dyn DownloadProgressPort>,
}

impl DownloadNginxUseCase {
    pub fn new(downloads: Arc<dyn DownloadProgressPort>) -> Self { Self { downloads } }

    pub async fn execute(&self, nginx_dir: &str) -> Result<String, ApplicationError> {
        if let Some(prog) = self.downloads.nginx_progress().await {
            if prog.state == "downloading" || prog.state == "extracting" {
                return Ok(prog.state);
            }
        }
        let dir = PathBuf::from(nginx_dir);
        self.downloads.start_nginx_download(&dir).await;
        Ok("pending".into())
    }
}
