use axum_test::TestServer;
use tempfile::TempDir;
use open_herd_lib::ports::http::server::build_router;
mod common;
use common::{make_container, make_server};

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
    assert!(catalog.iter().any(|e| e["major"] == "8.2"), "Catalog should include PHP 8.2");
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
    let container = make_container(&tmp);

    // Pre-registrar PHP 8.2 para que start_php_fpm pueda encontrarlo.
    container.php_version_repo.replace(vec![
        open_herd_lib::domain::ports::process_manager::PhpInstallation {
            major: "8.2".into(), version: "8.2.31".into(), binary_path: "php".into(),
        }
    ]).await;

    let server = TestServer::new(build_router(container)).unwrap();

    // Start: 200 si el binario existe, 500 si no (en CI). Nunca 404 ni cuelgue.
    let resp = server.post("/api/v1/php/versions/8.2/start").await;
    assert!(
        resp.status_code().is_success() || resp.status_code().as_u16() == 500,
        "start must respond with 2xx or 5xx, got {}", resp.status_code()
    );
    // Stop siempre 200
    server.post("/api/v1/php/versions/8.2/stop").await.assert_status_ok();
}
