use tempfile::TempDir;
use super::helpers::make_server;

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
