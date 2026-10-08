use async_trait::async_trait;
use std::path::Path;

use super::task::TaskState;

/// Progreso de una descarga en curso (nginx o una versión de PHP).
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub state: TaskState,
    pub message: String,
    pub percent: u8,
    pub error: Option<String>,
}

impl DownloadProgress {
    pub fn error(e: &str) -> Self {
        Self { state: TaskState::Error, message: e.into(), percent: 0, error: Some(e.into()) }
    }
}

/// Puerto de progreso de descargas — reemplaza `AppState.downloads`.
#[async_trait]
pub trait DownloadProgressPort: Send + Sync {
    /// Inicia la descarga de nginx si no hay una ya en curso. No bloquea.
    async fn start_nginx_download(&self, dest_dir: &Path);
    async fn nginx_progress(&self) -> Option<DownloadProgress>;
    /// Inicia la descarga de la versión PHP dada si no hay una ya en curso. No bloquea.
    async fn start_php_download(&self, major: &str, php_dir: &Path);
    async fn php_progress(&self, major: &str) -> Option<DownloadProgress>;
}
