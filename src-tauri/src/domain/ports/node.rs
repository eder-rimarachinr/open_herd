use async_trait::async_trait;
use std::path::PathBuf;

use crate::domain::{errors::InfrastructureError, node::{NodeReleaseInfo, NodeVersion}};

/// Who owns the active-version link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeManagerMode {
    /// nvm-windows is installed: versions live in `NVM_HOME`, the link is `NVM_SYMLINK`.
    NvmWindows,
    /// nvm (nvm-sh, Linux) is installed: versions live in `$NVM_DIR/versions/node`,
    /// the active one is nvm's `default` alias.
    Nvm,
    /// No nvm: versions live in `<base_dir>/node`, the link is `<base_dir>/node/current`.
    OpenHerd,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeInstall {
    pub version: NodeVersion,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct NodeEnvironment {
    pub mode: NodeManagerMode,
    /// Folder that holds one `v<version>` folder per install.
    pub store_dir: PathBuf,
    /// Junction / symlink on the PATH that points at the active version.
    pub link_path: PathBuf,
    /// Complete installs only, newest first.
    pub installed: Vec<NodeInstall>,
    pub active: Option<NodeVersion>,
    pub warnings: Vec<String>,
}

/// Installed Node versions and the system-wide active one. Changes the machine
/// outside `base_dir` (junctions, user PATH), so tests inject a double through
/// `SystemAdapters`.
#[async_trait]
pub trait NodeManagerPort: Send + Sync {
    async fn environment(&self) -> Result<NodeEnvironment, InfrastructureError>;
    async fn activate(&self, version: &NodeVersion) -> Result<(), InfrastructureError>;
    async fn uninstall(&self, version: &NodeVersion) -> Result<(), InfrastructureError>;
}

/// The release list published by nodejs.org.
#[async_trait]
pub trait NodeReleasesPort: Send + Sync {
    /// `refresh` bypasses the adapter's cache.
    async fn releases(&self, refresh: bool) -> Result<Vec<NodeReleaseInfo>, InfrastructureError>;
}
