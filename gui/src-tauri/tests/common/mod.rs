use axum_test::TestServer;
use std::sync::Arc;
use tempfile::TempDir;

use phpenv_gui_lib::daemon::{
    config::Config,
    server::build_router,
    state::AppState,
};

pub fn make_state(tmp: &TempDir) -> Arc<AppState> {
    let base = tmp.path().to_path_buf();
    std::fs::create_dir_all(&base).unwrap();
    let config = Config {
        api_addr: "127.0.0.1:0".into(),
        base_dir: base.to_string_lossy().into(),
        nginx_dir: base.join("nginx").to_string_lossy().into(),
        php_dir: base.join("php").to_string_lossy().into(),
        certs_dir: base.join("certs").to_string_lossy().into(),
        logs_dir: base.join("logs").to_string_lossy().into(),
        http_port: 8080,
        https_port: 8443,
        scanned_dirs: vec![],
        default_php: "8.2".into(),
        custom_php_dirs: vec![],
        os: "windows".into(),
    };
    AppState::new(base, config)
}

pub fn make_server(tmp: &TempDir) -> TestServer {
    let state = make_state(tmp);
    let router = build_router(state);
    TestServer::new(router).unwrap()
}
