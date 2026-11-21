#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use axum::http::StatusCode;
use serde_json::{json, Value};
use common::{make_server_with_composer, FakeComposerManager};

/// A default PHP no machine has installed, so the real detector (which also
/// scans XAMPP / Laragon folders) never finds it.
async fn use_missing_default_php(server: &axum_test::TestServer) {
    server.put("/api/v1/config").json(&json!({ "default_php": "5.1" })).await.assert_status_ok();
}

#[tokio::test]
async fn status_of_a_fresh_machine() {
    let tmp = tempfile::TempDir::new().unwrap();
    let server = make_server_with_composer(&tmp, FakeComposerManager::new(None));
    use_missing_default_php(&server).await;
    let body = server.get("/api/v1/composer").await.json::<Value>();
    assert_eq!(body["installed"], false);
    assert_eq!(body["version"], Value::Null);
    assert_eq!(body["default_php"], "5.1");
    assert_eq!(body["php_version"], Value::Null);
    assert_eq!(body["command_path"], r"C:\phpenv\bin\composer.bat");
    assert_eq!(body["latest"], "2.11.2", "no PHP to match: the newest release");
    assert_eq!(body["update_available"], false);
    assert_eq!(body["missing_extensions"], json!([]));
}

#[tokio::test]
async fn installed_composer_without_its_php_explains_why_it_cannot_run() {
    let tmp = tempfile::TempDir::new().unwrap();
    let server = make_server_with_composer(&tmp, FakeComposerManager::new(Some("2.11.2")));
    use_missing_default_php(&server).await;
    let body = server.get("/api/v1/composer").await.json::<Value>();
    assert_eq!(body["installed"], true);
    assert_eq!(body["version"], "2.11.2");
    let warnings = body["warnings"].as_array().unwrap();
    assert!(warnings.iter().any(|w| w.as_str().unwrap().contains("PHP 5.1")), "{warnings:?}");
}

#[tokio::test]
async fn install_needs_the_default_php() {
    let tmp = tempfile::TempDir::new().unwrap();
    let composer = FakeComposerManager::new(None);
    let server = make_server_with_composer(&tmp, composer.clone());
    use_missing_default_php(&server).await;
    let res = server.post("/api/v1/composer/install").expect_failure().await;
    assert_eq!(res.status_code(), StatusCode::CONFLICT);
    assert!(res.json::<Value>()["error"].as_str().unwrap().contains("5.1"));
    assert!(composer.command_php.lock().unwrap().is_none(), "nothing written");
}

#[tokio::test]
async fn progress_is_idle_before_any_install() {
    let tmp = tempfile::TempDir::new().unwrap();
    let server = make_server_with_composer(&tmp, FakeComposerManager::new(None));
    let body = server.get("/api/v1/composer/install/progress").await.json::<Value>();
    assert_eq!(body["state"], "idle");
    assert_eq!(body["percent"], 0);
}

#[tokio::test]
async fn enabling_extensions_needs_a_php_ini_managed_by_open_herd() {
    let tmp = tempfile::TempDir::new().unwrap();
    let server = make_server_with_composer(&tmp, FakeComposerManager::new(None));
    use_missing_default_php(&server).await;
    let res = server.post("/api/v1/composer/extensions").expect_failure().await;
    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn enabling_extensions_turns_them_on_in_php_ini() {
    let tmp = tempfile::TempDir::new().unwrap();
    let server = make_server_with_composer(&tmp, FakeComposerManager::new(None));
    use_missing_default_php(&server).await;
    let ini_dir = tmp.path().join("php").join("5.1");
    std::fs::create_dir_all(&ini_dir).unwrap();
    std::fs::write(ini_dir.join("php.ini"), ";extension=openssl\n;extension=mbstring\nextension=curl\n").unwrap();

    server.post("/api/v1/composer/extensions").await.assert_status_ok();

    let ini = std::fs::read_to_string(ini_dir.join("php.ini")).unwrap();
    for ext in ["openssl", "mbstring", "curl", "zip"] {
        assert!(ini.lines().any(|l| l == format!("extension={ext}")), "{ext} enabled:\n{ini}");
    }
}
