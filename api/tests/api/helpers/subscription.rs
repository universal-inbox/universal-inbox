use reqwest::Client;

use universal_inbox::subscription::SubscriptionInfo;

use crate::helpers::TestedApp;

pub async fn get_subscription_status_response(
    client: &Client,
    app: &TestedApp,
) -> reqwest::Response {
    client
        .get(format!("{}subscriptions/me", app.api_address))
        .send()
        .await
        .unwrap()
}

pub async fn get_subscription_status(client: &Client, app: &TestedApp) -> SubscriptionInfo {
    get_subscription_status_response(client, app)
        .await
        .json()
        .await
        .unwrap()
}
