use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;

use crate::domain::{errors::InfrastructureError, ports::php_ini::{PhpIniFile, PhpIniStore}};
use crate::infrastructure::{blocking, fs, state::AppState};

/// php.ini files under `config.php_dir/<major>/php.ini` (the PHP versions
/// Open Herd installed). Reads `php_dir` from the live config on every call.
pub struct FsPhpIniStore { state: Arc<AppState> }

impl FsPhpIniStore {
    pub fn new(state: Arc<AppState>) -> Self { Self { state } }

    fn path_for(&self, major: &str) -> PathBuf {
        PathBuf::from(&self.state.config.read().php_dir).join(major).join("php.ini")
    }
}

#[async_trait]
impl PhpIniStore for FsPhpIniStore {
    async fn read(&self, major: &str) -> Result<Option<PhpIniFile>, InfrastructureError> {
        let path = self.path_for(major);
        blocking::run(move || match std::fs::read_to_string(&path) {
            Ok(content) => Ok(Some(PhpIniFile { path: path.to_string_lossy().into_owned(), content })),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(InfrastructureError::Io(e)),
        }).await
    }

    async fn write(&self, major: &str, content: &str) -> Result<(), InfrastructureError> {
        let (path, content) = (self.path_for(major), content.to_owned());
        blocking::run(move || fs::atomic_write(&path, content.as_bytes()).map_err(InfrastructureError::Io)).await
    }
}
