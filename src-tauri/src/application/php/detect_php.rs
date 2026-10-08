use std::sync::Arc;
use crate::domain::{errors::ApplicationError, ports::process_manager::{PhpDetectorPort, PhpInstallation, PhpVersionRepository}};

pub struct DetectPhpUseCase {
    detector: Arc<dyn PhpDetectorPort>,
    versions: Arc<dyn PhpVersionRepository>,
}

impl DetectPhpUseCase {
    pub fn new(detector: Arc<dyn PhpDetectorPort>, versions: Arc<dyn PhpVersionRepository>) -> Self {
        Self { detector, versions }
    }

    pub async fn execute(&self) -> Result<Vec<PhpInstallation>, ApplicationError> {
        let installs = self.detector.detect().await;
        self.versions.replace(installs.clone()).await;
        Ok(installs)
    }
}
