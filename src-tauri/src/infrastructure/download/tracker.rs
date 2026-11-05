use async_trait::async_trait;
use std::path::Path;
use std::sync::Arc;

use super::DownloadState;
use crate::domain::{node::NodeVersion, ports::{download::{DownloadProgress, DownloadProgressPort}, task::TaskState}};

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
            current.as_ref().is_some_and(|p| p.state.is_active())
        };
        if already_active { return; }
        super::download_nginx(dest_dir, self.state.clone());
    }

    async fn nginx_progress(&self) -> Option<DownloadProgress> {
        self.state.nginx.lock().clone()
    }

    async fn start_php_download(&self, major: &str, php_dir: &Path) {
        let already_active = self.state.php.lock().get(major)
            .is_some_and(|p| p.state.is_active());
        if already_active { return; }
        super::download_php(major, php_dir, self.state.clone());
    }

    async fn php_progress(&self, major: &str) -> Option<DownloadProgress> {
        self.state.php.lock().get(major).cloned()
    }

    async fn start_node_download(&self, version: &NodeVersion, dest_dir: &Path) {
        {
            // Check and claim under one lock: two quick POSTs can't both start a
            // download, and the GUI's first poll already sees `pending`.
            let mut node = self.state.node.lock();
            if node.get(version.as_str()).is_some_and(|p| p.state.is_active()) { return; }
            node.insert(version.as_str().to_string(), DownloadProgress {
                state: TaskState::Pending, message: "Starting…".into(), percent: 0, error: None,
            });
        }
        super::node::download_node(version, dest_dir, self.state.clone());
    }

    async fn node_progress(&self, version: &NodeVersion) -> Option<DownloadProgress> {
        self.state.node.lock().get(version.as_str()).cloned()
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
        assert!(tracker.node_progress(&NodeVersion::parse("22.19.0").unwrap()).await.is_none());
    }

    #[tokio::test]
    async fn node_progress_is_pending_right_after_start() {
        // current-thread runtime: the spawned download has not run yet, so only
        // a state recorded before spawning can be seen here. Without it the GUI's
        // first poll reads "idle" and stops following the download.
        let tracker = DownloadTracker::new();
        let v = NodeVersion::parse("0.0.1").unwrap();
        tracker.start_node_download(&v, Path::new("/nonexistent")).await;
        let p = tracker.node_progress(&v).await.expect("progress recorded before the task runs");
        assert_eq!(p.state, TaskState::Pending);
    }

    #[tokio::test]
    async fn second_start_call_is_a_no_op_while_first_is_downloading() {
        let tracker = DownloadTracker::new();
        // Simulate an in-flight download without touching the filesystem/network.
        *tracker.state.nginx.lock() = Some(DownloadProgress {
            state: TaskState::Downloading, message: "…".into(), percent: 10, error: None,
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
