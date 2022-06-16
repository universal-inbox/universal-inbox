use crate::helpers::app_address;
use rstest::*;

#[rstest]
#[tokio::test]
async fn health_check_works(#[future] app_address: String) {
    let response = reqwest::Client::new()
        .get(&format!("{}/ping", app_address.await))
        .send()
        .await
        .expect("Failed to execute request.");

    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}
