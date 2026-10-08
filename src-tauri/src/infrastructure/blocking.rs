use crate::domain::errors::InfrastructureError;

/// Runs blocking work (file I/O, child processes, `sleep`s) on Tokio's blocking
/// pool so adapters never stall an async worker — those workers also serve the
/// GUI's status polling.
pub async fn run<T, F>(f: F) -> Result<T, InfrastructureError>
where
    F: FnOnce() -> Result<T, InfrastructureError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .unwrap_or_else(|e| Err(InfrastructureError::ProcessFailed(format!("blocking task failed: {e}"))))
}
