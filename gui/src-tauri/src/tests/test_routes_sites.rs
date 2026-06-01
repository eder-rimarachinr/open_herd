use axum::http::StatusCode;
use tempfile::TempDir;

use super::helpers::make_server;

fn site_body(tmp: &TempDir) -> serde_json::Value {
    serde_json::json!({
        "domain": "myapp.test",
        "path": tmp.path().to_str().unwrap()
    })
}

// ── CRUD ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_sites_empty() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let resp = server.get("/api/v1/sites").await;
    resp.assert_status_ok();
    assert_eq!(resp.json::<serde_json::Value>(), serde_json::json!([]));
}

#[tokio::test]
async fn create_site_returns_201() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let resp = server.post("/api/v1/sites").json(&site_body(&tmp)).await;
    resp.assert_status(StatusCode::CREATED);
    let body = resp.json::<serde_json::Value>();
    assert_eq!(body["domain"], "myapp.test");
    assert!(body["id"].as_str().unwrap().len() > 0);
    assert_eq!(body["ssl_enabled"], false);
}

#[tokio::test]
async fn create_site_missing_domain_returns_400() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let resp = server
        .post("/api/v1/sites")
        .json(&serde_json::json!({ "path": "/some/path" }))
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn create_site_missing_path_returns_400() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let resp = server
        .post("/api/v1/sites")
        .json(&serde_json::json!({ "domain": "x.test" }))
        .await;
    resp.assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn get_site_returns_correct_data() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let id = server
        .post("/api/v1/sites").json(&site_body(&tmp)).await
        .json::<serde_json::Value>()["id"].as_str().unwrap().to_string();

    let resp = server.get(&format!("/api/v1/sites/{id}")).await;
    resp.assert_status_ok();
    assert_eq!(resp.json::<serde_json::Value>()["domain"], "myapp.test");
}

#[tokio::test]
async fn get_site_unknown_id_returns_404() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let resp = server.get("/api/v1/sites/nonexistent-id").await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn update_site_php_version() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let id = server
        .post("/api/v1/sites").json(&site_body(&tmp)).await
        .json::<serde_json::Value>()["id"].as_str().unwrap().to_string();

    let resp = server
        .put(&format!("/api/v1/sites/{id}"))
        .json(&serde_json::json!({ "php_version": "8.1" }))
        .await;
    resp.assert_status_ok();
    assert_eq!(resp.json::<serde_json::Value>()["php_version"], "8.1");
}

#[tokio::test]
async fn update_site_unknown_id_returns_404() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let resp = server
        .put("/api/v1/sites/bad-id")
        .json(&serde_json::json!({ "php_version": "8.1" }))
        .await;
    resp.assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn delete_site_removes_it() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let id = server
        .post("/api/v1/sites").json(&site_body(&tmp)).await
        .json::<serde_json::Value>()["id"].as_str().unwrap().to_string();

    server.delete(&format!("/api/v1/sites/{id}")).await.assert_status_ok();

    // Should now be gone
    server.get(&format!("/api/v1/sites/{id}")).await
        .assert_status(StatusCode::NOT_FOUND);

    // And list should be empty
    let list = server.get("/api/v1/sites").await.json::<serde_json::Value>();
    assert_eq!(list, serde_json::json!([]));
}

#[tokio::test]
async fn delete_site_unknown_id_returns_404() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    server.delete("/api/v1/sites/bad-id").await
        .assert_status(StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn list_sites_sorted_by_domain() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);

    for domain in &["zebra.test", "alpha.test", "middle.test"] {
        server.post("/api/v1/sites")
            .json(&serde_json::json!({ "domain": domain, "path": tmp.path() }))
            .await;
    }

    let list = server.get("/api/v1/sites").await.json::<Vec<serde_json::Value>>();
    let domains: Vec<&str> = list.iter().map(|s| s["domain"].as_str().unwrap()).collect();
    assert_eq!(domains, vec!["alpha.test", "middle.test", "zebra.test"]);
}

// ── SSL ───────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn enable_ssl_sets_flag() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let id = server
        .post("/api/v1/sites").json(&site_body(&tmp)).await
        .json::<serde_json::Value>()["id"].as_str().unwrap().to_string();

    server.post(&format!("/api/v1/sites/{id}/ssl")).await.assert_status_ok();

    let site = server.get(&format!("/api/v1/sites/{id}")).await
        .json::<serde_json::Value>();
    assert_eq!(site["ssl_enabled"], true);
}

#[tokio::test]
async fn disable_ssl_clears_flag() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let id = server
        .post("/api/v1/sites").json(&site_body(&tmp)).await
        .json::<serde_json::Value>()["id"].as_str().unwrap().to_string();

    server.post(&format!("/api/v1/sites/{id}/ssl")).await;
    server.delete(&format!("/api/v1/sites/{id}/ssl")).await.assert_status_ok();

    let site = server.get(&format!("/api/v1/sites/{id}")).await
        .json::<serde_json::Value>();
    assert_eq!(site["ssl_enabled"], false);
}

#[tokio::test]
async fn ssl_progress_returns_done_state() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let id = server
        .post("/api/v1/sites").json(&site_body(&tmp)).await
        .json::<serde_json::Value>()["id"].as_str().unwrap().to_string();

    let prog = server
        .get(&format!("/api/v1/sites/{id}/ssl/progress")).await
        .json::<serde_json::Value>();
    assert_eq!(prog["state"], "done");
}

// ── Scan & bulk ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn scan_with_no_dirs_returns_400() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    server.post("/api/v1/sites/scan").await
        .assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn scan_nonexistent_dir_returns_400() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);

    // Add a non-existent dir to config
    server.put("/api/v1/config")
        .json(&serde_json::json!({ "scanned_dirs": ["/does/not/exist"] }))
        .await;

    server.post("/api/v1/sites/scan").await
        .assert_status(StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn scan_finds_subdirectories() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);

    // Create a subdirectory to scan
    let proj_dir = tmp.path().join("projects");
    std::fs::create_dir_all(proj_dir.join("mysite")).unwrap();

    server.put("/api/v1/config")
        .json(&serde_json::json!({ "scanned_dirs": [proj_dir.to_str().unwrap()] }))
        .await;

    let resp = server.post("/api/v1/sites/scan").await;
    resp.assert_status_ok();
    let list = resp.json::<Vec<serde_json::Value>>();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["domain"], "mysite.test");
}

#[tokio::test]
async fn scan_does_not_duplicate_existing_sites() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);

    let proj_dir = tmp.path().join("projects");
    std::fs::create_dir_all(proj_dir.join("mysite")).unwrap();

    server.put("/api/v1/config")
        .json(&serde_json::json!({ "scanned_dirs": [proj_dir.to_str().unwrap()] }))
        .await;

    server.post("/api/v1/sites/scan").await;
    server.post("/api/v1/sites/scan").await; // second scan

    let list = server.get("/api/v1/sites").await.json::<Vec<serde_json::Value>>();
    assert_eq!(list.len(), 1, "Duplicate site created on second scan");
}

#[tokio::test]
async fn bulk_add_sites() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let path = tmp.path().to_str().unwrap();

    let resp = server.post("/api/v1/sites/bulk")
        .json(&serde_json::json!([
            { "domain": "a.test", "path": path },
            { "domain": "b.test", "path": path },
        ]))
        .await;
    resp.assert_status_ok();
    let list = resp.json::<Vec<serde_json::Value>>();
    assert_eq!(list.len(), 2);
}

// ── Persistence ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn sites_persisted_to_disk() {
    let tmp = TempDir::new().unwrap();
    {
        let server = make_server(&tmp);
        server.post("/api/v1/sites").json(&site_body(&tmp)).await;
    } // server dropped

    // Load a fresh server from the same dir
    let server2 = make_server(&tmp);
    let list = server2.get("/api/v1/sites").await.json::<Vec<serde_json::Value>>();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["domain"], "myapp.test");
}
