#![allow(clippy::unwrap_used, clippy::expect_used)]

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

// ── php.ini editor ────────────────────────────────────────────────────────────

fn write_php_ini(tmp: &TempDir, content: &str) -> std::path::PathBuf {
    let dir = tmp.path().join("php").join("8.2");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("php.ini");
    std::fs::write(&path, content).unwrap();
    path
}

#[tokio::test]
async fn php_ini_of_a_version_without_one_is_404() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    server.get("/api/v1/php/versions/8.2/ini").await.assert_status(axum::http::StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn php_ini_is_read_through_the_api() {
    let tmp = TempDir::new().unwrap();
    write_php_ini(&tmp, "extension=curl\r\nmemory_limit = 256M\r\n");
    let server = make_server(&tmp);
    let body = server.get("/api/v1/php/versions/8.2/ini").await.json::<serde_json::Value>();
    let ext = body["extensions"].as_array().unwrap().iter().find(|e| e["name"] == "curl").unwrap().clone();
    assert_eq!(ext["enabled"], true);
    let mem = body["settings"].as_array().unwrap().iter().find(|s| s["key"] == "memory_limit").unwrap().clone();
    assert_eq!(mem["value"], "256M");
}

#[tokio::test]
async fn invalid_php_ini_value_is_a_400_and_nothing_is_written() {
    let tmp = TempDir::new().unwrap();
    let path = write_php_ini(&tmp, "memory_limit = 128M\n");
    let server = make_server(&tmp);
    let resp = server.put("/api/v1/php/versions/8.2/ini")
        .json(&serde_json::json!({ "settings": [{ "key": "memory_limit", "value": "lots" }] }))
        .await;
    resp.assert_status(axum::http::StatusCode::BAD_REQUEST); // was 500
    assert!(resp.json::<serde_json::Value>()["error"].as_str().unwrap().contains("memory_limit"));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "memory_limit = 128M\n");
}

#[tokio::test]
async fn php_ini_update_edits_only_what_was_asked_and_keeps_crlf() {
    let tmp = TempDir::new().unwrap();
    let path = write_php_ini(&tmp, "extension=xmlrpc\r\nextension=xml\r\nmemory_limit = 128M\r\n");
    let server = make_server(&tmp);
    // Same shape the GUI sends: full objects with category/label/hint.
    let resp = server.put("/api/v1/php/versions/8.2/ini")
        .json(&serde_json::json!({
            "extensions": [{ "name": "xml", "enabled": false, "category": "misc" }],
            "settings":   [{ "key": "memory_limit", "value": "512M", "label": "Memory Limit", "hint": "" }],
        }))
        .await;
    resp.assert_status_ok();
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "extension=xmlrpc\r\n;extension=xml\r\nmemory_limit = 512M\r\n",
        "xmlrpc must survive disabling xml",
    );
    let body = resp.json::<serde_json::Value>();
    assert!(body["extensions"].as_array().unwrap().iter().any(|e| e["name"] == "xml" && e["enabled"] == false));
}
