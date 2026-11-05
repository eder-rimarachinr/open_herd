use std::sync::Arc;

use crate::domain::{errors::ApplicationError, ports::node::{NodeEnvironment, NodeManagerPort}};

pub struct GetNodeStatusUseCase {
    manager: Arc<dyn NodeManagerPort>,
}

impl GetNodeStatusUseCase {
    pub fn new(manager: Arc<dyn NodeManagerPort>) -> Self { Self { manager } }

    pub async fn execute(&self) -> Result<NodeEnvironment, ApplicationError> {
        Ok(self.manager.environment().await?)
    }
}
