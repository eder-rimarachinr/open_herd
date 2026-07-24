use async_trait::async_trait;
use parking_lot::RwLock;
use std::sync::Arc;

use crate::domain::ports::process_manager::{PhpInstallation, PhpVersionRepository};

/// Caché en memoria de la última detección de PHP. Vive en el `AppContainer`,
/// no en `AppState` — sustituye a `AppState.php_versions`.
pub struct InMemoryPhpVersionRepository {
    versions: RwLock<Vec<PhpInstallation>>,
}

impl InMemoryPhpVersionRepository {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { versions: RwLock::new(Vec::new()) })
    }
}

#[async_trait]
impl PhpVersionRepository for InMemoryPhpVersionRepository {
    async fn replace(&self, installs: Vec<PhpInstallation>) {
        *self.versions.write() = installs;
    }

    async fn list(&self) -> Vec<PhpInstallation> {
        self.versions.read().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install(major: &str) -> PhpInstallation {
        PhpInstallation { major: major.into(), version: format!("{}.0", major), binary_path: "php".into() }
    }

    #[tokio::test]
    async fn list_is_empty_before_any_replace() {
        let repo = InMemoryPhpVersionRepository::new();
        assert!(repo.list().await.is_empty());
    }

    #[tokio::test]
    async fn replace_overwrites_previous_list() {
        let repo = InMemoryPhpVersionRepository::new();
        repo.replace(vec![install("8.1")]).await;
        assert_eq!(repo.list().await.len(), 1);

        repo.replace(vec![install("8.2"), install("8.3")]).await;
        let listed = repo.list().await;
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].major, "8.2");
    }
}
