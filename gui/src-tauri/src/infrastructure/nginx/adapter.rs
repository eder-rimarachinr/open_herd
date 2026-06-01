use async_trait::async_trait;
use std::sync::Arc;

use crate::infrastructure::{
    nginx::{process as ng, vhost_config},
    state::AppState,
};
use crate::domain::{errors::InfrastructureError, ports::web_server::WebServerPort, site::entity::Site};
use super::super::persistence::site_mapper;

pub struct NginxAdapter { state: Arc<AppState> }
impl NginxAdapter { pub fn new(state: Arc<AppState>) -> Self { Self { state } } }

#[async_trait]
impl WebServerPort for NginxAdapter {
    async fn create_vhost(&self, site: &Site) -> Result<(), InfrastructureError> {
        let config = self.state.config.read();
        let nginx_dir = config.nginx_dir.clone(); let http_port = config.http_port; let certs_dir = config.certs_dir.clone();
        drop(config);
        let legacy = site_mapper::to_legacy(site);
        let result = if site.ssl.is_enabled() { vhost_config::generate_with_certs(&legacy, &nginx_dir, http_port, Some(&certs_dir)) } else { vhost_config::generate(&legacy, &nginx_dir, http_port) };
        result.map_err(|e| InfrastructureError::Io(std::io::Error::other(e)))
    }

    async fn remove_vhost(&self, site: &Site) -> Result<(), InfrastructureError> {
        let nginx_dir = self.state.config.read().nginx_dir.clone();
        vhost_config::remove(&site_mapper::to_legacy(site), &nginx_dir);
        Ok(())
    }

    async fn reload(&self) -> Result<(), InfrastructureError> {
        // En Windows, `nginx -s reload` abre un named event basado en el PID
        // del proceso maestro. Si nginx no está corriendo, el evento no existe
        // y el comando falla con "OpenEvent failed". Skip reload si no está activo.
        if !ng::is_running(&self.state.nginx_proc) {
            return Ok(());
        }
        ng::reload(&self.state).map_err(|e| InfrastructureError::ProcessFailed(e))
    }

    async fn is_running(&self) -> bool { ng::is_running(&self.state.nginx_proc) }

    async fn start(&self) -> Result<(), InfrastructureError> {
        let state = self.state.clone(); let nginx_proc = self.state.nginx_proc.clone();
        tokio::task::spawn_blocking(move || ng::start(&state, &nginx_proc).map_err(|e| InfrastructureError::ProcessFailed(e)))
            .await.unwrap_or_else(|_| Err(InfrastructureError::ProcessFailed("spawn_blocking panicked".into())))
    }

    async fn stop(&self) -> Result<(), InfrastructureError> {
        let state = self.state.clone(); let nginx_proc = self.state.nginx_proc.clone();
        tokio::task::spawn_blocking(move || ng::stop(&state, &nginx_proc).map_err(|e| InfrastructureError::ProcessFailed(e)))
            .await.unwrap_or_else(|_| Err(InfrastructureError::ProcessFailed("spawn_blocking panicked".into())))
    }
}
