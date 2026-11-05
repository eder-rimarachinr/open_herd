use std::sync::Arc;

use crate::domain::{
    errors::{ApplicationError, DomainError},
    node::NodeVersion,
    ports::node::{NodeEnvironment, NodeManagerPort},
};

/// Makes an installed version the system-wide active one.
pub struct ActivateNodeUseCase {
    manager: Arc<dyn NodeManagerPort>,
}

impl ActivateNodeUseCase {
    pub fn new(manager: Arc<dyn NodeManagerPort>) -> Self { Self { manager } }

    /// Returns the environment after the switch.
    pub async fn execute(&self, raw_version: &str) -> Result<NodeEnvironment, ApplicationError> {
        let version = NodeVersion::parse(raw_version)?;
        let env = self.manager.environment().await?;
        if !env.installed.iter().any(|i| i.version == version) {
            return Err(DomainError::NodeVersionNotInstalled(version.to_string()).into());
        }
        self.manager.activate(&version).await?;
        Ok(self.manager.environment().await?)
    }
}
