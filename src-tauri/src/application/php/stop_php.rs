use std::sync::Arc;
use crate::domain::{errors::ApplicationError, ports::process_manager::PhpProcessPort};

pub struct StopPhpUseCase {
    process: Arc<dyn PhpProcessPort>,
}

impl StopPhpUseCase {
    pub fn new(process: Arc<dyn PhpProcessPort>) -> Self { Self { process } }

    pub async fn execute(&self, major: &str) -> Result<(), ApplicationError> {
        self.process.stop(major).await.map_err(ApplicationError::Infrastructure)
    }
}
