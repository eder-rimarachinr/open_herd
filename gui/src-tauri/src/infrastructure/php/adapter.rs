use async_trait::async_trait;
use std::sync::Arc;

use crate::infrastructure::{dto, php::process as php_mgr, state::AppState};
use crate::domain::{errors::InfrastructureError, ports::process_manager::{PhpInstallation, PhpProcessPort}};

pub struct PhpProcessAdapter { state: Arc<AppState> }
impl PhpProcessAdapter { pub fn new(state: Arc<AppState>) -> Self { Self { state } } }

impl PhpProcessAdapter {
    fn to_legacy(inst: &PhpInstallation) -> dto::PhpVersion {
        dto::PhpVersion {
            version: inst.version.clone(), major: inst.major.clone(),
            binary_path: inst.binary_path.clone(), fpm_binary: inst.binary_path.clone(),
            fastcgi_addr: format!("127.0.0.1:{}", inst.fastcgi_port()), installed: true, running: false,
        }
    }
}

#[async_trait]
impl PhpProcessPort for PhpProcessAdapter {
    async fn start(&self, installation: &PhpInstallation) -> Result<(), InfrastructureError> {
        php_mgr::start(&self.state, &self.state.php_proc, &Self::to_legacy(installation))
            .map_err(|e| InfrastructureError::ProcessFailed(e))
    }
    async fn stop(&self, major: &str) -> Result<(), InfrastructureError> {
        php_mgr::stop(&self.state, &self.state.php_proc, major)
            .map_err(|e| InfrastructureError::ProcessFailed(e))
    }
    async fn stop_all(&self) -> Result<(), InfrastructureError> {
        php_mgr::stop_all(&self.state, &self.state.php_proc); Ok(())
    }
    async fn is_running(&self, major: &str) -> bool { php_mgr::is_running(&self.state.php_proc, major) }
    async fn running_majors(&self) -> Vec<String> { php_mgr::running_versions(&self.state.php_proc) }
}
