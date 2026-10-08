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
