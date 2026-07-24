use crate::domain::ports::process_manager::PhpInstallation;
use crate::infrastructure::dto;

/// Convierte una instalación de dominio al DTO legacy que consume la GUI.
/// Antes duplicado en `PhpProcessAdapter::to_legacy` y `SystemPhpDetector::detect`.
pub fn to_legacy(inst: &PhpInstallation) -> dto::PhpVersion {
    dto::PhpVersion {
        version: inst.version.clone(),
        major: inst.major.clone(),
        binary_path: inst.binary_path.clone(),
        fpm_binary: inst.binary_path.clone(),
        fastcgi_addr: format!("127.0.0.1:{}", inst.fastcgi_port()),
        installed: true,
        running: false,
    }
}
