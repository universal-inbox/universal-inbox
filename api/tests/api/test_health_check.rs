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
    assert_eq!(
        json!({ "cache": "healthy", "database": "healthy" }).to_string(),
        body
    );
}
