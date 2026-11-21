//! Composer: install `composer.phar` and keep the `composer` command pointing
//! at the default PHP version.

pub mod enable_composer_extensions;
pub mod get_composer_status;
pub mod install_composer;
pub mod sync_composer_command;

mod tests;

use crate::domain::ports::process_manager::{PhpDetectorPort, PhpInstallation};

/// The detected install of PHP `major` with an absolute binary path (a bare
/// `php` found on the PATH can't be written into the command).
pub(crate) async fn default_php(detector: &dyn PhpDetectorPort, major: &str) -> Option<PhpInstallation> {
    detector.detect().await.into_iter()
        .find(|p| p.major == major && std::path::Path::new(&p.binary_path).is_absolute())
}
