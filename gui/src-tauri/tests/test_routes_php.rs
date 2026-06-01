use tempfile::TempDir;
mod common;
use common::make_server;

#[tokio::test]
async fn php_versions_returns_array() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let body = server.get("/api/v1/php/versions").await.json::<serde_json::Value>();
    assert!(body.is_array(), "Expected array of PHP versions");
}

#[tokio::test]
async fn php_catalog_returns_known_versions() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let catalog = server.get("/api/v1/php/catalog").await
        .json::<Vec<serde_json::Value>>();
    assert!(!catalog.is_empty(), "Catalog should not be empty");

    // All entries must have required fields
    for entry in &catalog {
        assert!(entry.get("major").is_some(),        "missing major");
        assert!(entry.get("latest_patch").is_some(), "missing latest_patch");
        assert!(entry.get("installed").is_some(),    "missing installed");
        assert!(entry.get("end_of_life").is_some(),  "missing end_of_life");
    }
}

#[tokio::test]
async fn php_catalog_contains_php82() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let catalog = server.get("/api/v1/php/catalog").await
        .json::<Vec<serde_json::Value>>();
    let has_82 = catalog.iter().any(|e| e["major"] == "8.2");
    assert!(has_82, "Catalog should include PHP 8.2");
}

#[tokio::test]
async fn php_detect_returns_catalog() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let resp = server.post("/api/v1/php/detect").await;
    resp.assert_status_ok();
    assert!(resp.json::<serde_json::Value>().is_array());
}

#[tokio::test]
async fn php_install_progress_returns_done_for_unknown() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let body = server.get("/api/v1/php/install/8.2/progress").await
        .json::<serde_json::Value>();
    assert_eq!(body["major"], "8.2");
    assert!(body.get("state").is_some());
    assert!(body.get("percent").is_some());
}

#[tokio::test]
async fn php_start_stop_return_ok() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    server.post("/api/v1/php/versions/8.2/start").await.assert_status_ok();
    server.post("/api/v1/php/versions/8.2/stop").await.assert_status_ok();
}
