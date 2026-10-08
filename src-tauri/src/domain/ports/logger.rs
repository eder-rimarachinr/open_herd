/// Puerto de log del daemon — reemplaza `AppState.daemon_log` / `AppState::log`.
/// Síncrono: es un buffer en memoria con salida a stderr, no hay E/S de red.
pub trait LoggerPort: Send + Sync {
    fn log(&self, message: String);
    /// Las últimas `limit` entradas, en orden cronológico (más antigua primero).
    fn recent(&self, limit: usize) -> Vec<String>;
}
