use crate::application::best_effort;
use std::sync::Arc;
use crate::domain::{
    errors::ApplicationError,
    ports::{process_manager::PhpProcessPort, web_server::WebServerPort},
};

pub struct StopServicesUseCase {
    php_proc:   Arc<dyn PhpProcessPort>,
    web_server: Arc<dyn WebServerPort>,
}

impl StopServicesUseCase {
    pub fn new(php_proc: Arc<dyn PhpProcessPort>, web_server: Arc<dyn WebServerPort>) -> Self {
        Self { php_proc, web_server }
    }

    pub async fn execute(&self) -> Result<(), ApplicationError> {
        best_effort(self.php_proc.stop_all().await, format_args!("PHP stop_all warning"));
        best_effort(self.web_server.stop().await, format_args!("Nginx stop warning"));
        Ok(())
    }
}
