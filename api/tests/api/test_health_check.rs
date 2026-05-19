use rstest::*;
use serde_json::json;

use crate::helpers::{tested_app, TestedApp};

#[rstest]
#[tokio::test]
async fn health_check_works(#[future] tested_app: TestedApp) {
    let response = reqwest::Client::new()
        .get(format!("{}/ping", tested_app.await.app_address))
        .send()
        .await
        .expect("Failed to execute request.");

    assert!(response.status().is_success());
    let body = response.text().await.expect("Failed to parse JSON result");
    // Smoke tests and uptime monitors (including the dev-loop in
    // `CLAUDE.md`: `curl http://localhost:${API_PORT}/ping | grep healthy`)
    // depend on this exact JSON shape — universal-inbox-bkj.18 preserved it
    // while removing the per-request transaction.
    assert_eq!(
        json!({ "cache": "healthy", "database": "healthy" }).to_string(),
        body
    );
}

/// `/ping` is unauthenticated by design, so an attacker (or a noisy uptime
/// monitor) could previously exhaust the database connection pool by issuing
/// concurrent health checks (each opened a real Postgres transaction). The
/// fix replaces the transaction with a pooled `SELECT 1` *and* installs a
/// per-IP governor limiter. This test asserts the limiter is wired and
/// returns 429 once the budget is exhausted — `PING_RATE_LIMIT_PER_MINUTE`
/// is currently 60.
#[rstest]
#[tokio::test]
async fn test_ping_is_ip_rate_limited(#[future] tested_app: TestedApp) {
    let app = tested_app.await;
    let client = reqwest::Client::new();
    let url = format!("{}/ping", app.app_address);

    // Drain the per-minute budget for `127.0.0.1` (the test client's IP).
    // We send 65 requests to safely overshoot the 60/min budget even if a
    // handful are already consumed by harness wiring.
    let mut saw_429 = false;
    let mut last_status = None;
    for _ in 0..65 {
        let status = client
            .get(&url)
            .send()
            .await
            .expect("Failed to execute /ping request")
            .status();
        last_status = Some(status);
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            saw_429 = true;
            break;
        }
    }

    assert!(
        saw_429,
        "Expected /ping to return 429 Too Many Requests once the per-IP \
         budget was exhausted; last observed status was {:?}",
        last_status
    );
}
