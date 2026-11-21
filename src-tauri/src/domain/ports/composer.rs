use async_trait::async_trait;
use std::path::{Path, PathBuf};

use crate::domain::{composer::ComposerRelease, errors::InfrastructureError};

/// The `composer.phar` Open Herd downloaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerInstall {
    /// `None` when the phar is there but Open Herd did not record its version
    /// (copied in by hand).
    pub version: Option<String>,
    /// Oldest PHP it runs on (`PHP_VERSION_ID`), when known.
    pub min_php: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct ComposerEnvironment {
    /// `<base_dir>/composer/composer.phar`.
    pub phar_path: PathBuf,
    /// The `composer` command: `<base_dir>/bin/composer.bat` on Windows,
    /// `<base_dir>/bin/composer` on Linux.
    pub command_path: PathBuf,
    /// Folder put on the user's PATH (Windows) or linked from `~/.local/bin` (Linux).
    pub bin_dir: PathBuf,
    pub installed: Option<ComposerInstall>,
    /// The PHP binary the command currently runs, read back from it.
    pub command_php: Option<PathBuf>,
    /// PATH problems: another Composer comes first, the folder is not on the PATH…
    pub warnings: Vec<String>,
}

/// The `composer` command on the user's machine. Changes it outside
/// `base_dir` (user PATH, `~/.local/bin`) and runs PHP, so tests inject a
/// double through `SystemAdapters`.
#[async_trait]
pub trait ComposerManagerPort: Send + Sync {
    async fn environment(&self) -> Result<ComposerEnvironment, InfrastructureError>;
    /// (Re)writes the command so it runs the phar with `php_binary`, and makes
    /// sure new terminals find it (user PATH once / `~/.local/bin` link).
    async fn write_command(&self, php_binary: &Path) -> Result<(), InfrastructureError>;
    /// Extensions `php_binary` loads (`php -m`), lowercase.
    async fn php_modules(&self, php_binary: &Path) -> Result<Vec<String>, InfrastructureError>;
}

/// The release list published by getcomposer.org.
#[async_trait]
pub trait ComposerReleasesPort: Send + Sync {
    async fn releases(&self) -> Result<Vec<ComposerRelease>, InfrastructureError>;
}
