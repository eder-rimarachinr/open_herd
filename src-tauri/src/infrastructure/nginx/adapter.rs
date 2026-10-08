use async_trait::async_trait;
use std::sync::Arc;

use crate::infrastructure::{
    blocking,
    nginx::{process as ng, vhost_config},
    state::AppState,
};
use crate::domain::{
    errors::InfrastructureError,
    ports::{logger::LoggerPort, web_server::{WebServerPort, WebServerStatus}},
    site::entity::Site,
};
use super::super::persistence::site_mapper;

pub struct NginxAdapter { state: Arc<AppState>, logger: Arc<dyn LoggerPort> }
impl NginxAdapter {
    pub fn new(state: Arc<AppState>, logger: Arc<dyn LoggerPort>) -> Self {
        Self { state, logger }
    }

    /// Runs one of the blocking `nginx::process` operations off the async workers.
    async fn process_op(
        &self,
        op: fn(&AppState, &ng::NginxProcess, &dyn LoggerPort) -> Result<(), String>,
    ) -> Result<(), InfrastructureError> {
        let (state, logger) = (self.state.clone(), self.logger.clone());
        blocking::run(move || {
            op(&state, &state.nginx_proc, logger.as_ref()).map_err(InfrastructureError::ProcessFailed)
        }).await
    }
}

#[async_trait]
impl WebServerPort for NginxAdapter {
    async fn create_vhost(&self, site: &Site) -> Result<(), InfrastructureError> {
        let (nginx_dir, ports, certs_dir) = {
            let config = self.state.config.read();
            let ports = vhost_config::ListenPorts { http: config.http_port, https: config.https_port };
            (config.nginx_dir.clone(), ports, config.certs_dir.clone())
        };
        let legacy = site_mapper::to_legacy(site);
        let ssl    = site.ssl.is_enabled();
        blocking::run(move || {
            let result = if ssl { vhost_config::generate_with_certs(&legacy, &nginx_dir, ports, Some(&certs_dir)) } else { vhost_config::generate(&legacy, &nginx_dir, ports) };
            result.map_err(|e| InfrastructureError::Io(std::io::Error::other(e)))
        }).await
    }

    async fn remove_vhost(&self, site: &Site) -> Result<(), InfrastructureError> {
        let nginx_dir = self.state.config.read().nginx_dir.clone();
        let legacy    = site_mapper::to_legacy(site);
        blocking::run(move || { vhost_config::remove(&legacy, &nginx_dir); Ok(()) }).await
    }

    async fn reload(&self) -> Result<(), InfrastructureError> { self.process_op(ng::reload).await }

    async fn is_running(&self) -> bool { self.state.nginx_proc.is_running() }

    async fn start(&self) -> Result<(), InfrastructureError> { self.process_op(ng::start).await }

    async fn stop(&self) -> Result<(), InfrastructureError> { self.process_op(ng::stop).await }

    async fn status(&self) -> WebServerStatus {
        let snap = self.state.nginx_proc.snapshot();
        WebServerStatus { running: snap.running, version: snap.version, pid: snap.pid }
    }
}
