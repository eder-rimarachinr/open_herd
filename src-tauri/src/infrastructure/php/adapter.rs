use async_trait::async_trait;
use std::sync::Arc;

use crate::infrastructure::{blocking, php::{process as php_mgr, version_mapper}, state::AppState};
use crate::domain::{
    errors::InfrastructureError,
    ports::{logger::LoggerPort, process_manager::{PhpInstallation, PhpProcessPort}},
};

pub struct PhpProcessAdapter { state: Arc<AppState>, logger: Arc<dyn LoggerPort> }
impl PhpProcessAdapter {
    pub fn new(state: Arc<AppState>, logger: Arc<dyn LoggerPort>) -> Self {
        Self { state, logger }
    }
}

#[async_trait]
impl PhpProcessPort for PhpProcessAdapter {
    async fn start(&self, installation: &PhpInstallation) -> Result<(), InfrastructureError> {
        let (state, logger) = (self.state.clone(), self.logger.clone());
        let version = version_mapper::to_legacy(installation);
        blocking::run(move || {
            php_mgr::start(&state.php_proc, logger.as_ref(), &version).map_err(InfrastructureError::ProcessFailed)
        }).await
    }
    async fn stop(&self, major: &str) -> Result<(), InfrastructureError> {
        let (state, logger) = (self.state.clone(), self.logger.clone());
        let major = major.to_owned();
        blocking::run(move || {
            php_mgr::stop(&state.php_proc, logger.as_ref(), &major).map_err(InfrastructureError::ProcessFailed)
        }).await
    }
    async fn stop_all(&self) -> Result<(), InfrastructureError> {
        let (state, logger) = (self.state.clone(), self.logger.clone());
        blocking::run(move || { php_mgr::stop_all(&state.php_proc, logger.as_ref()); Ok(()) }).await
    }
    async fn is_running(&self, major: &str) -> bool { php_mgr::is_running(&self.state.php_proc, major) }
    async fn running_majors(&self) -> Vec<String> { php_mgr::running_versions(&self.state.php_proc) }
}
