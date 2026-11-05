//! Node.js adapters: the nvm-aware version manager and the nodejs.org release index.

#[cfg(windows)]
pub mod manager;
pub mod nvm;
pub mod releases;
pub mod user_path;

// TODO(linux): manage Node on Linux (tarballs + a symlink on the PATH). Until
// then every operation reports that it is not supported.
#[cfg(not(windows))]
pub mod unsupported {
    use async_trait::async_trait;
    use crate::domain::{
        errors::InfrastructureError,
        node::NodeVersion,
        ports::node::{NodeEnvironment, NodeManagerPort},
    };

    pub struct UnsupportedNodeManager;

    fn unsupported() -> InfrastructureError {
        InfrastructureError::ProcessFailed("Node management is only supported on Windows for now".into())
    }

    #[async_trait]
    impl NodeManagerPort for UnsupportedNodeManager {
        async fn environment(&self) -> Result<NodeEnvironment, InfrastructureError> { Err(unsupported()) }
        async fn activate(&self, _: &NodeVersion) -> Result<(), InfrastructureError> { Err(unsupported()) }
        async fn uninstall(&self, _: &NodeVersion) -> Result<(), InfrastructureError> { Err(unsupported()) }
    }
}
