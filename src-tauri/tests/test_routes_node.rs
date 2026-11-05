#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::http::StatusCode;
use serde_json::{json, Value};
use tempfile::TempDir;
mod common;
use common::{make_server_with_node, FakeNodeManager};

#[tokio::test]
async fn status_lists_fake_installs() {
    let tmp = TempDir::new().unwrap();
    let server = make_server_with_node(&tmp, FakeNodeManager::new(&["22.19.0", "24.21.0"], Some("22.19.0")), false);
    let body = server.get("/api/v1/node").await.json::<Value>();
    assert_eq!(body["mode"], "nvm-windows");
    assert_eq!(body["active"], "22.19.0");
    assert_eq!(body["link_path"], r"C:\nvm4w\nodejs");
    let versions: Vec<&str> = body["installed"].as_array().unwrap().iter().map(|i| i["version"].as_str().unwrap()).collect();
    assert_eq!(versions, ["22.19.0", "24.21.0"]);
}

#[tokio::test]
async fn activate_switches_and_returns_status() {
    let tmp = TempDir::new().unwrap();
    let node = FakeNodeManager::new(&["22.19.0", "24.21.0"], Some("22.19.0"));
    let server = make_server_with_node(&tmp, node.clone(), false);
    let body = server.post("/api/v1/node/versions/24.21.0/activate").await.json::<Value>();
    assert_eq!(body["active"], "24.21.0");
    assert_eq!(node.active.lock().unwrap().as_ref().map(|v| v.as_str().to_string()).as_deref(), Some("24.21.0"));
}

#[tokio::test]
async fn activate_unknown_version_is_404() {
    let tmp = TempDir::new().unwrap();
    let server = make_server_with_node(&tmp, FakeNodeManager::new(&["22.19.0"], None), false);
    let res = server.post("/api/v1/node/versions/18.0.0/activate").expect_failure().await;
    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
    assert!(res.json::<Value>()["error"].is_string());
}

#[tokio::test]
async fn activate_with_traversal_is_400() {
    let tmp = TempDir::new().unwrap();
    let server = make_server_with_node(&tmp, FakeNodeManager::new(&["22.19.0"], None), false);
    let res = server.post("/api/v1/node/versions/..%5C..%5Cx/activate").expect_failure().await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
    let res = server.post("/api/v1/node/versions/22.19/activate").expect_failure().await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn uninstall_active_is_409_and_other_is_removed() {
    let tmp = TempDir::new().unwrap();
    let node = FakeNodeManager::new(&["22.19.0", "24.21.0"], Some("22.19.0"));
    let server = make_server_with_node(&tmp, node.clone(), false);
    let res = server.delete("/api/v1/node/versions/22.19.0").expect_failure().await;
    assert_eq!(res.status_code(), StatusCode::CONFLICT);
    server.delete("/api/v1/node/versions/24.21.0").await.assert_status_ok();
    assert_eq!(node.installed.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn releases_list_marks_installed() {
    let tmp = TempDir::new().unwrap();
    let server = make_server_with_node(&tmp, FakeNodeManager::new(&["24.21.0"], Some("24.21.0")), false);
    let body = server.get("/api/v1/node/releases").await.json::<Vec<Value>>();
    assert_eq!(body.len(), 2);
    assert_eq!(body[0]["version"], "24.21.0");
    assert_eq!(body[0]["end_of_life"], false);
    assert_eq!(body[0]["major"], 24);
    assert_eq!(body[0]["lts"], "Krypton");
    assert_eq!(body[0]["installed"], true);
    assert_eq!(body[0]["active"], true);
}

#[tokio::test]
async fn releases_offline_is_502_while_status_ok() {
    let tmp = TempDir::new().unwrap();
    let server = make_server_with_node(&tmp, FakeNodeManager::new(&["22.19.0"], None), true);
    let res = server.get("/api/v1/node/releases?refresh=1").expect_failure().await;
    assert_eq!(res.status_code(), StatusCode::BAD_GATEWAY);
    assert!(res.json::<Value>()["error"].is_string());
    server.get("/api/v1/node").await.assert_status_ok();
}

#[tokio::test]
async fn install_invalid_body_is_400_and_installed_is_done() {
    let tmp = TempDir::new().unwrap();
    let server = make_server_with_node(&tmp, FakeNodeManager::new(&["22.19.0"], None), false);
    let res = server.post("/api/v1/node/install").json(&json!({ "version": "../x" })).expect_failure().await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
    let res = server.post("/api/v1/node/install").json(&json!({ "nope": 1 })).expect_failure().await;
    assert_eq!(res.status_code(), StatusCode::BAD_REQUEST);
    let body = server.post("/api/v1/node/install").json(&json!({ "version": "22.19.0" })).await.json::<Value>();
    assert_eq!(body["state"], "done");
}

#[tokio::test]
async fn install_progress_is_idle_before_any_download() {
    let tmp = TempDir::new().unwrap();
    let server = make_server_with_node(&tmp, FakeNodeManager::new(&[], None), false);
    let body = server.get("/api/v1/node/install/24.21.0/progress").await.json::<Value>();
    assert_eq!(body["version"], "24.21.0");
    assert_eq!(body["state"], "idle");
}

#[tokio::test]
async fn releases_include_old_lts_lines_marked_end_of_life() {
    let tmp = TempDir::new().unwrap();
    let server = make_server_with_node(&tmp, FakeNodeManager::new(&[], None), false);
    let body = server.get("/api/v1/node/releases").await.json::<Vec<Value>>();
    let node10 = body.iter().find(|r| r["major"] == 10).expect("Node 10 is offered");
    assert_eq!(node10["version"], "10.24.1");
    assert_eq!(node10["lts"], "Dubnium");
    assert_eq!(node10["end_of_life"], true);
    assert_eq!(node10["installed"], false);
}
