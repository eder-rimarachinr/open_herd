use async_trait::async_trait;
use crate::domain::errors::InfrastructureError;

/// Instalación de PHP detectada en el sistema — incluye info de infraestructura
/// (binary_path) necesaria para iniciar el proceso.
#[derive(Debug, Clone)]
pub struct PhpInstallation {
    /// Versión major.minor, ej. "8.2"
    pub major: String,
    /// Versión completa, ej. "8.2.18"
    pub version: String,
    /// Ruta al binario php.exe / php
    pub binary_path: String,
}

impl PhpInstallation {
    /// Puerto FastCGI calculado a partir del major, ej. "8.2" → 9082
    pub fn fastcgi_port(&self) -> u16 {
        let parts: Vec<u16> = self.major
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();
        match parts.as_slice() {
            [major, minor, ..] => 9000 + major * 10 + minor,
            [major]            => 9000 + major * 10,
            _                  => 9082,
        }
    }
}

/// Puerto de gestión de procesos PHP-CGI / PHP-FPM.
#[async_trait]
pub trait PhpProcessPort: Send + Sync {
    async fn start(&self, installation: &PhpInstallation) -> Result<(), InfrastructureError>;
    /// Detiene la versión major dada (ej. "8.2").
    async fn stop(&self, major: &str) -> Result<(), InfrastructureError>;
    async fn stop_all(&self) -> Result<(), InfrastructureError>;
    async fn is_running(&self, major: &str) -> bool;
    /// Lista de versiones major actualmente en ejecución.
    async fn running_majors(&self) -> Vec<String>;
}

/// Puerto de detección de instalaciones PHP en el sistema.
#[async_trait]
pub trait PhpDetectorPort: Send + Sync {
    /// Escanea el sistema y devuelve las instalaciones encontradas.
    /// También puede actualizar cualquier caché interno.
    async fn detect(&self) -> Vec<PhpInstallation>;
}

/// Puerto de caché de versiones PHP detectadas. Reemplaza el campo
/// `AppState.php_versions` — el detector escribe aquí tras cada detección,
/// los handlers leen de aquí en vez de tocar AppState directamente.
#[async_trait]
pub trait PhpVersionRepository: Send + Sync {
    /// Reemplaza la caché completa con el resultado de una nueva detección.
    async fn replace(&self, installs: Vec<PhpInstallation>);
    /// Última lista detectada (vacía si `detect()` nunca se ejecutó).
    async fn list(&self) -> Vec<PhpInstallation>;
}
