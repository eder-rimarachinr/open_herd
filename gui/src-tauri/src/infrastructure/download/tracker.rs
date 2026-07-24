use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

use super::DownloadState;
use crate::domain::ports::download::{DownloadProgress, DownloadProgressPort};

/// Adaptador de progreso de descargas — envuelve el `DownloadState` legacy
/// (mutex compartido, poblado desde hilos de fondo) detrás del puerto de dominio.
pub struct DownloadTracker {
    state: Arc<DownloadState>,
}

impl DownloadTracker {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { state: DownloadState::new() })
    }
}

#[async_trait]
impl DownloadProgressPort for DownloadTracker {
    async fn start_nginx_download(&self, dest_dir: &Path) {
        let already_active = {
            let current = self.state.nginx.lock();
            current.as_ref().is_some_and(|p| p.state == "downloading" || p.state == "extracting")
        };
        if already_active { return; }
        super::download_nginx(dest_dir, self.state.clone());
    }

    async fn nginx_progress(&self) -> Option<DownloadProgress> {
        self.state.nginx.lock().clone()
    }

    async fn start_php_download(&self, major: &str, php_dir: &Path) {
        let already_active = self.state.php.lock().get(major)
            .is_some_and(|p| p.state == "downloading" || p.state == "extracting");
        if already_active { return; }
        super::download_php(major, php_dir, self.state.clone());
    }

    async fn php_progress(&self, major: &str) -> Option<DownloadProgress> {
        self.state.php.lock().get(major).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn no_progress_before_any_download_starts() {
        let tracker = DownloadTracker::new();
        assert!(tracker.nginx_progress().await.is_none());
        assert!(tracker.php_progress("8.2").await.is_none());
    }

    #[tokio::test]
    async fn second_start_call_is_a_no_op_while_first_is_downloading() {
        let tracker = DownloadTracker::new();
        // Simulate an in-flight download without touching the filesystem/network.
        *tracker.state.nginx.lock() = Some(DownloadProgress {
            state: "downloading".into(), message: "…".into(), percent: 10, error: None,
        });
        tracker.start_nginx_download(Path::new("/nonexistent")).await;
        // Still the same progress snapshot — a second background download was not spawned
        // (if it had been, `fetch_zip` would eventually overwrite state with an error for
        // the bogus path, which this test would catch if we waited — instead we assert the
        // synchronous guard itself: percent is untouched immediately after the call).
        match tracker.nginx_progress().await {
            Some(p) => assert_eq!(p.percent, 10),
            None => panic!("expected the simulated in-flight progress to still be present"),
        }
    }
}
