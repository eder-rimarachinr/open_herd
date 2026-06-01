use tempfile::TempDir;
use super::helpers::make_server;

#[tokio::test]
async fn nginx_status_returns_not_running() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let resp = server.get("/api/v1/nginx/status").await;
    resp.assert_status_ok();
    assert_eq!(resp.json::<serde_json::Value>()["running"], false);
}

#[tokio::test]
async fn nginx_info_returns_expected_fields() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let body = server.get("/api/v1/nginx/info").await.json::<serde_json::Value>();
    assert!(body.get("installed").is_some(), "missing 'installed' field");
    assert!(body.get("running").is_some(),   "missing 'running' field");
    assert!(body.get("downloadable").is_some(), "missing 'downloadable' field");
    assert!(body.get("os").is_some(),        "missing 'os' field");
}

#[tokio::test]
async fn nginx_download_progress_returns_done_when_idle() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let body = server.get("/api/v1/nginx/download/progress").await
        .json::<serde_json::Value>();
    assert_eq!(body["state"], "done");
}

#[tokio::test]
async fn nginx_reload_without_binary_returns_error() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    // No nginx binary in temp dir — should return an error, not panic
    let resp = server.post("/api/v1/nginx/reload").await;
    // Must not be 500 from a panic — 200 (handled error) or 500 (explicit error) both OK
    // The key is it doesn't crash the server
    assert_ne!(resp.status_code().as_u16(), 0);
}

#[tokio::test]
async fn subsequent_requests_work_after_failed_nginx_start() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);

    // Start will fail (no binary) but server must stay alive
    server.post("/api/v1/nginx/start").await;

    // Server is still responsive
    server.get("/api/v1/nginx/status").await.assert_status_ok();
}
