use std::sync::Arc;
use crate::domain::{errors::ApplicationError, ports::process_manager::{PhpDetectorPort, PhpInstallation}};

pub struct DetectPhpUseCase {
    detector: Arc<dyn PhpDetectorPort>,
}

impl DetectPhpUseCase {
    pub fn new(detector: Arc<dyn PhpDetectorPort>) -> Self { Self { detector } }

    pub async fn execute(&self) -> Result<Vec<PhpInstallation>, ApplicationError> {
        Ok(self.detector.detect().await)
    }
}
