use std::sync::Arc;
use crate::application::best_effort;
use crate::domain::{errors::ApplicationError, ports::web_server::WebServerPort};

/// Stops and starts the web server so it picks up settings that only apply at
/// start, such as the ports it listens on (start regenerates every vhost and
/// the main config from the current settings). A failed stop is logged and
/// the start is still attempted; the start's outcome is the result.
pub struct RestartNginxUseCase { web_server: Arc<dyn WebServerPort> }

impl RestartNginxUseCase {
    pub fn new(web_server: Arc<dyn WebServerPort>) -> Self { Self { web_server } }

    pub async fn execute(&self) -> Result<(), ApplicationError> {
        best_effort(self.web_server.stop().await, format_args!("nginx stop before restart"));
        self.web_server.start().await.map_err(ApplicationError::Infrastructure)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{errors::InfrastructureError, site::entity::Site};
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Recorder { calls: Mutex<Vec<&'static str>>, fail_start: bool }

    #[async_trait]
    impl WebServerPort for Recorder {
        async fn create_vhost(&self, _: &Site) -> Result<(), InfrastructureError> { Ok(()) }
        async fn remove_vhost(&self, _: &Site) -> Result<(), InfrastructureError> { Ok(()) }
        async fn reload(&self) -> Result<(), InfrastructureError> { Ok(()) }
        async fn is_running(&self) -> bool { true }
        async fn stop(&self) -> Result<(), InfrastructureError> {
            self.calls.lock().unwrap().push("stop");
            Err(InfrastructureError::ProcessFailed("already stopped".into()))
        }
        async fn start(&self) -> Result<(), InfrastructureError> {
            self.calls.lock().unwrap().push("start");
            if self.fail_start { Err(InfrastructureError::ProcessFailed("bind() to 0.0.0.0:80 failed".into())) } else { Ok(()) }
        }
    }

    #[tokio::test]
    async fn stops_then_starts_even_if_stop_fails() {
        let web = Arc::new(Recorder::default());
        RestartNginxUseCase::new(web.clone()).execute().await.unwrap();
        assert_eq!(*web.calls.lock().unwrap(), ["stop", "start"]);
    }

    #[tokio::test]
    async fn a_failed_start_is_reported() {
        let web = Arc::new(Recorder { fail_start: true, ..Default::default() });
        let err = RestartNginxUseCase::new(web).execute().await.unwrap_err();
        assert!(err.to_string().contains("bind()"));
    }
}
