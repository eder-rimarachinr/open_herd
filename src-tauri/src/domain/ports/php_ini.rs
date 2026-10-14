use async_trait::async_trait;
use crate::domain::errors::InfrastructureError;

/// The php.ini of one PHP version, as text.
#[derive(Debug, Clone)]
pub struct PhpIniFile {
    /// Where it lives (shown to the user).
    pub path: String,
    pub content: String,
}

/// Raw access to the php.ini of the PHP versions Open Herd manages. Knows where
/// the file is and how to write it safely; knows nothing about its syntax.
#[async_trait]
pub trait PhpIniStore: Send + Sync {
    /// `Ok(None)` when that version has no php.ini (not installed by Open Herd).
    async fn read(&self, major: &str) -> Result<Option<PhpIniFile>, InfrastructureError>;
    /// Replaces the whole file.
    async fn write(&self, major: &str, content: &str) -> Result<(), InfrastructureError>;
}
