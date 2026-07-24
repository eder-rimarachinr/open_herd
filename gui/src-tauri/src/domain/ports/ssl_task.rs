/// Progreso de una emisión/revocación de certificado SSL en curso para un sitio.
#[derive(Debug, Clone)]
pub struct SslTaskProgress {
    pub state: String,
    pub message: String,
    pub error: Option<String>,
}

/// Puerto de seguimiento de progreso SSL por sitio — reemplaza `AppState.ssl_tasks`.
/// Deliberadamente síncrono: es un mapa en memoria, no hay E/S.
pub trait SslTaskPort: Send + Sync {
    fn set(&self, site_id: &str, state: &str, message: &str, error: Option<String>);
    fn get(&self, site_id: &str) -> Option<SslTaskProgress>;
    fn remove(&self, site_id: &str);
}
