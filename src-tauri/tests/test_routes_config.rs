#![allow(clippy::unwrap_used, clippy::expect_used)]

use tempfile::TempDir;
mod common;
use common::make_server;

#[tokio::test]
async fn get_config_returns_defaults() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let body = server.get("/api/v1/config").await.json::<serde_json::Value>();
    assert_eq!(body["default_php"], "8.2");
    assert_eq!(body["http_port"], 8080);
    assert!(body["scanned_dirs"].is_array());
}

#[tokio::test]
async fn update_config_default_php() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let body = server.put("/api/v1/config")
        .json(&serde_json::json!({ "default_php": "8.1" }))
        .await
        .json::<serde_json::Value>();
    assert_eq!(body["default_php"], "8.1");
}

#[tokio::test]
async fn update_config_scanned_dirs() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let dirs = vec!["/projects/a", "/projects/b"];
    let body = server.put("/api/v1/config")
        .json(&serde_json::json!({ "scanned_dirs": dirs }))
        .await
        .json::<serde_json::Value>();
    assert_eq!(body["scanned_dirs"][0], "/projects/a");
    assert_eq!(body["scanned_dirs"][1], "/projects/b");
}

#[tokio::test]
async fn update_config_persists_on_get() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    server.put("/api/v1/config")
        .json(&serde_json::json!({ "default_php": "7.4" }))
        .await;
    let body = server.get("/api/v1/config").await.json::<serde_json::Value>();
    assert_eq!(body["default_php"], "7.4");
}

#[tokio::test]
async fn update_config_partial_leaves_other_fields() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);

    // Update only default_php
    server.put("/api/v1/config")
        .json(&serde_json::json!({ "default_php": "8.0" }))
        .await;

    let body = server.get("/api/v1/config").await.json::<serde_json::Value>();
    assert_eq!(body["default_php"], "8.0");
    assert_eq!(body["http_port"], 8080, "http_port should be unchanged");
}

#[tokio::test]
async fn status_reports_no_warnings_on_clean_start() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let body = server.get("/api/v1/status").await.json::<serde_json::Value>();
    assert_eq!(body["warnings"], serde_json::json!([]));
}

#[tokio::test]
async fn status_reports_corrupt_sites_json() {
    let tmp = TempDir::new().unwrap();
    std::fs::write(tmp.path().join("sites.json"), "{ truncated").unwrap();
    let server = make_server(&tmp);
    let body = server.get("/api/v1/status").await.json::<serde_json::Value>();
    let warnings = body["warnings"].as_array().unwrap();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].as_str().unwrap().contains("sites.json"));
}

#[tokio::test]
async fn update_config_rejects_invalid_values_and_keeps_previous_config() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    for body in [
        serde_json::json!({ "http_port": 70000 }),        // would have wrapped to 4464
        serde_json::json!({ "http_port": 0 }),
        serde_json::json!({ "https_port": "443" }),
        serde_json::json!({ "https_port": 8080 }),         // same as http_port
        serde_json::json!({ "default_php": "latest" }),
        serde_json::json!({ "scanned_dirs": ["C:/ok", 42] }),
    ] {
        server.put("/api/v1/config").json(&body).await
            .assert_status(axum::http::StatusCode::BAD_REQUEST);
    }
    let cfg = server.get("/api/v1/config").await.json::<serde_json::Value>();
    assert_eq!(cfg["http_port"], 8080);
    assert_eq!(cfg["https_port"], 8443);
    assert_eq!(cfg["default_php"], "8.2");
    assert!(!tmp.path().join("config.json").exists(), "nothing invalid may reach disk");
}

#[tokio::test]
async fn update_config_persists_valid_ports() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    server.put("/api/v1/config")
        .json(&serde_json::json!({ "http_port": 8081, "https_port": 8444 })).await
        .assert_status_ok();
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(tmp.path().join("config.json")).unwrap()).unwrap();
    assert_eq!(saved["http_port"], 8081);
    assert_eq!(saved["https_port"], 8444);
}

// ── Applying port changes to a running nginx ──────────────────────────────────

mod hot_apply {
    use std::sync::{Arc, Mutex};
    use axum::http::StatusCode;
    use tempfile::TempDir;
    use super::common::{make_server_with_web_server, FakeWebServer};

    fn web(running: bool, fail_starts: u32) -> Arc<FakeWebServer> {
        Arc::new(FakeWebServer { running, fail_starts: Mutex::new(fail_starts), ..Default::default() })
    }
    fn saved_http_port(tmp: &TempDir) -> u64 {
        let cfg: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(tmp.path().join("config.json")).unwrap()).unwrap();
        cfg["http_port"].as_u64().unwrap()
    }

    #[tokio::test]
    async fn port_change_restarts_a_running_nginx() {
        let tmp = TempDir::new().unwrap();
        let nginx = web(true, 0);
        let server = make_server_with_web_server(&tmp, nginx.clone());
        server.put("/api/v1/config").json(&serde_json::json!({ "http_port": 8081 })).await.assert_status_ok();
        assert_eq!(*nginx.calls.lock().unwrap(), ["stop", "start"]);
        assert_eq!(saved_http_port(&tmp), 8081);
    }

    #[tokio::test]
    async fn stopped_nginx_is_not_started_by_a_port_change() {
        let tmp = TempDir::new().unwrap();
        let nginx = web(false, 0);
        let server = make_server_with_web_server(&tmp, nginx.clone());
        server.put("/api/v1/config").json(&serde_json::json!({ "http_port": 8081 })).await.assert_status_ok();
        assert!(nginx.calls.lock().unwrap().is_empty());
        assert_eq!(saved_http_port(&tmp), 8081, "applied on the next start");
    }

    #[tokio::test]
    async fn non_port_changes_do_not_restart_nginx() {
        let tmp = TempDir::new().unwrap();
        let nginx = web(true, 0);
        let server = make_server_with_web_server(&tmp, nginx.clone());
        server.put("/api/v1/config")
            .json(&serde_json::json!({ "default_php": "8.3", "http_port": 8080 })) // 8080 = current
            .await.assert_status_ok();
        assert!(nginx.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn busy_port_rolls_back_to_the_previous_ports() {
        let tmp = TempDir::new().unwrap();
        let nginx = web(true, 1); // the start on the new port fails, the rollback start works
        let server = make_server_with_web_server(&tmp, nginx.clone());

        let resp = server.put("/api/v1/config").json(&serde_json::json!({ "http_port": 8081 })).await;

        resp.assert_status(StatusCode::CONFLICT);
        let error = resp.json::<serde_json::Value>()["error"].as_str().unwrap().to_string();
        assert!(error.contains("8081") && error.contains("8080") && error.contains("address in use"), "{error}");
        assert_eq!(*nginx.calls.lock().unwrap(), ["stop", "start", "stop", "start"]);
        assert_eq!(saved_http_port(&tmp), 8080, "config.json restored");
        let live = server.get("/api/v1/config").await.json::<serde_json::Value>();
        assert_eq!(live["http_port"], 8080, "live config restored");
    }

    #[tokio::test]
    async fn failed_rollback_is_reported_as_a_server_error() {
        let tmp = TempDir::new().unwrap();
        let nginx = web(true, 2); // new ports fail, and the old ones too
        let server = make_server_with_web_server(&tmp, nginx);
        let resp = server.put("/api/v1/config").json(&serde_json::json!({ "http_port": 8081 })).await;
        resp.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
        assert!(resp.json::<serde_json::Value>()["error"].as_str().unwrap().contains("also failed"));
    }
}
