use async_trait::async_trait;
use std::{future::Future, path::Path, pin::Pin};

use super::task::TaskState;
use crate::domain::{composer::ComposerRelease, node::NodeVersion};

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
    /// Inicia la descarga de esa versión de Node en `dest_dir` (la carpeta
    /// `v<versión>` final) si no hay una ya en curso. No bloquea.
    async fn start_node_download(&self, version: &NodeVersion, dest_dir: &Path);
    async fn node_progress(&self, version: &NodeVersion) -> Option<DownloadProgress>;
    /// Starts downloading that `composer.phar` into `dest_dir` (verified
    /// against getcomposer.org's checksum) unless one is already in flight,
    /// then runs `after` while the state is `configuring`. No bloquea.
    async fn start_composer_download(&self, release: &ComposerRelease, dest_dir: &Path, after: PostInstall);
    async fn composer_progress(&self) -> Option<DownloadProgress>;
}

/// Work that finishes an install once its files are in place (e.g. writing
/// the `composer` command). An `Err` message fails the task.
pub type PostInstall = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>> + Send>;
