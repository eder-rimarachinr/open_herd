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
        if let Err(e) = self.php_proc.stop_all().await {
            eprintln!("[services] PHP stop_all warning: {}", e);
        }
        if let Err(e) = self.web_server.stop().await {
            eprintln!("[services] Nginx stop warning: {}", e);
        }
        Ok(())
    }
}
