use std::sync::Arc;
use crate::domain::{errors::ApplicationError, ports::web_server::WebServerPort};

pub struct StartNginxUseCase { web_server: Arc<dyn WebServerPort> }
impl StartNginxUseCase {
    pub fn new(web_server: Arc<dyn WebServerPort>) -> Self { Self { web_server } }
    pub async fn execute(&self) -> Result<(), ApplicationError> {
        self.web_server.start().await.map_err(ApplicationError::Infrastructure)
    }
}
