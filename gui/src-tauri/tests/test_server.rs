/// Tests that catch infrastructure-level bugs (port binding, routing).
use axum_test::TestServer;
use tempfile::TempDir;

mod common;
use common::make_server;
// infrastructure imports available via make_server/make_state from common

// ── Port binding ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn server_binds_and_responds() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);
    let resp = server.get("/api/v1/status").await;
    resp.assert_status_ok();
}

#[tokio::test]
async fn two_independent_servers_can_start() {
    // Verifies that two test servers (different ports) don't conflict.
    // This would have caught the port-already-in-use panic during hot-reload.
    let tmp1 = TempDir::new().unwrap();
    let tmp2 = TempDir::new().unwrap();
    let s1 = make_server(&tmp1);
    let s2 = make_server(&tmp2);

    s1.get("/api/v1/status").await.assert_status_ok();
    s2.get("/api/v1/status").await.assert_status_ok();
}

// ── Route registration ────────────────────────────────────────────────────────

/// This test would have caught the {id} vs :id routing bug —
/// every parameterized route must return non-404 with a real ID.
#[tokio::test]
async fn all_site_param_routes_are_registered() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);

    // First create a site so we have a valid ID
    let resp = server
        .post("/api/v1/sites")
        .json(&serde_json::json!({ "domain": "test.test", "path": tmp.path().to_str().unwrap() }))
        .await;
    resp.assert_status(axum::http::StatusCode::CREATED);
    let id = resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Every parameterized route must NOT return 404
    let routes = vec![
        ("GET",    format!("/api/v1/sites/{id}")),
        ("GET",    format!("/api/v1/sites/{id}/ssl/progress")),
        ("GET",    format!("/api/v1/sites/{id}/info")),
        ("POST",   format!("/api/v1/sites/{id}/ssl")),
        ("POST",   format!("/api/v1/sites/{id}/open-folder")),
        ("POST",   format!("/api/v1/sites/{id}/refresh-config")),
    ];

    for (method, path) in routes {
        let resp = match method {
            "GET"  => server.get(&path).await,
            "POST" => server.post(&path).await,
            _      => unreachable!(),
        };
        assert_ne!(
            resp.status_code(),
            axum::http::StatusCode::NOT_FOUND,
            "Route {} {} returned 404 — likely missing or wrong param syntax",
            method, path
        );
    }
}

#[tokio::test]
async fn all_php_param_routes_are_registered() {
    let tmp = TempDir::new().unwrap();
    let server = make_server(&tmp);

    let routes = vec![
        ("GET",  "/api/v1/php/install/8.2/progress".to_string()),
        ("POST", "/api/v1/php/versions/8.2/start".to_string()),
        ("POST", "/api/v1/php/versions/8.2/stop".to_string()),
    ];

    for (method, path) in routes {
        let resp = match method {
            "GET"  => server.get(&path).await,
            "POST" => server.post(&path).await,
            _      => unreachable!(),
        };
        assert_ne!(
            resp.status_code(),
            axum::http::StatusCode::NOT_FOUND,
            "Route {} {} returned 404",
            method, path
        );
    }
}
