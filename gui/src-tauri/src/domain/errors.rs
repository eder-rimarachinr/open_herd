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
}

/// Errores de casos de uso — agregan contexto de aplicación sobre los de capas inferiores.
#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("{0}")]
    Domain(#[from] DomainError),
    #[error("Error de infraestructura: {0}")]
    Infrastructure(#[from] InfrastructureError),
}
