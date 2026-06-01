use std::sync::Arc;
use crate::domain::{errors::ApplicationError, ports::web_server::WebServerPort};

pub struct StopNginxUseCase { web_server: Arc<dyn WebServerPort> }
impl StopNginxUseCase {
    pub fn new(web_server: Arc<dyn WebServerPort>) -> Self { Self { web_server } }
    pub async fn execute(&self) -> Result<(), ApplicationError> {
        self.web_server.stop().await.map_err(ApplicationError::Infrastructure)
    }
}
