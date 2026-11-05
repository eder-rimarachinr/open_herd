use thiserror::Error;

/// Errores que violan reglas de negocio — no dependen de IO ni de infra.
#[derive(Debug, Error, PartialEq)]
pub enum DomainError {
    #[error("El dominio debe terminar en .test")]
    InvalidTld,
    #[error("Dominio inválido: {0}")]
    InvalidDomain(String),
    #[error("La ruta no es válida: {0}")]
    InvalidPath(String),
    #[error("Sitio no encontrado: {0}")]
    SiteNotFound(String),
    #[error("El dominio {0} ya existe")]
    DomainAlreadyExists(String),
    #[error("SSL ya está activo para {0}")]
    SslAlreadyActive(String),
    #[error("No se puede activar SSL: {0}")]
    SslCannotEnable(String),
    /// El repositorio no pudo leer o escribir (disco, registro dañado). No es un
    /// error del usuario: se expone como 500, no como 4xx.
    #[error("Error de persistencia: {0}")]
    Persistence(String),
    /// Esa versión de PHP no tiene php.ini gestionado por Open Herd (404).
    #[error("php.ini no encontrado para PHP {0}: instala esa versión primero")]
    PhpIniNotFound(String),
    /// Cambio de php.ini rechazado por la validación (error del usuario, 400).
    #[error("Cambio de php.ini no válido: {0}")]
    InvalidPhpIniChange(String),
    /// Versión de Node mal formada (400). Se valida antes de tocar rutas.
    #[error("Versión de Node no válida: {0}")]
    InvalidNodeVersion(String),
    /// Esa versión de Node no está instalada (404).
    #[error("Node {0} no está instalado")]
    NodeVersionNotInstalled(String),
    /// No se puede desinstalar la versión activa (409).
    #[error("Node {0} es la versión activa: activa otra antes de desinstalarla")]
    NodeVersionActive(String),
    /// Hay una descarga en curso para esa versión (409).
    #[error("Node {0} se está descargando")]
    NodeVersionBusy(String),
}

/// Errores originados en la capa de infraestructura (IO, procesos, red).
#[derive(Debug, Error)]
pub enum InfrastructureError {
    #[error("Error de E/S: {0}")]
    Io(#[from] std::io::Error),
    #[error("Proceso falló: {0}")]
    ProcessFailed(String),
    #[error("Error de serialización: {0}")]
    Serialization(String),
    #[error("Descarga fallida: {0}")]
    DownloadFailed(String),
    #[error("Recurso no encontrado: {0}")]
    NotFound(String),
    /// El estado de la máquina impide la operación sin riesgo (409), p. ej. el
    /// enlace de Node es una carpeta real que no vamos a borrar.
    #[error("{0}")]
    Conflict(String),
}

/// Errores de casos de uso — agregan contexto de aplicación sobre los de capas inferiores.
#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("{0}")]
    Domain(#[from] DomainError),
    #[error("Error de infraestructura: {0}")]
    Infrastructure(#[from] InfrastructureError),
}
