use chrono::Utc;
use pretty_assertions::assert_eq;
use rstest::*;
use slack_morphism::prelude::*;

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    slack::post_signed_slack_event,
};

// Integration tests here only verify the *wiring* between Actix-web extractors,
// the signing-secret `web::Data`, and the handler. The cryptographic logic itself
// is covered by fast unit tests in `api/src/routes/webhook.rs::tests`.

#[rstest]
#[tokio::test]
async fn test_receive_slack_url_verification_event(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;

    let response = post_signed_slack_event(
        &app.client,
        &app.app.api_address,
        &SlackPushEvent::UrlVerification(SlackUrlVerificationEvent {
            challenge: "test challenge".to_string(),
        }),
    )
    .await;

    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    assert_eq!(body, r#"{"challenge":"test challenge"}"#);
}

#[rstest]
#[tokio::test]
async fn test_receive_slack_ignored_event(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;

    let response = post_signed_slack_event(
        &app.client,
        &app.app.api_address,
        &SlackPushEvent::AppRateLimited(SlackAppRateLimitedEvent {
            team_id: "T123456".to_string(),
            minute_rate_limited: Utc::now().into(),
            api_app_id: "A123456".to_string(),
        }),
    )
    .await;

    // Return no error to Slack
    assert_eq!(response.status(), 200);
}

#[rstest]
#[tokio::test]
async fn test_receive_slack_ignored_push_event_callback(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;

    let response = post_signed_slack_event(
        &app.client,
        &app.app.api_address,
        &SlackPushEvent::EventCallback(SlackPushEventCallback {
            team_id: "T123456".into(),
            api_app_id: "A123456".into(),
            event: SlackEventCallbackBody::AppHomeOpened(SlackAppHomeOpenedEvent {
                user: "U123456".into(),
                channel: "C123456".into(),
                tab: Some("home".into()),
                view: None,
            }),
            event_id: "Ev123456".into(),
            event_time: Utc::now().into(),
            event_context: None,
            authed_users: None,
            authorizations: None,
        }),
    )
    .await;

    // Return no error to Slack
    assert_eq!(response.status(), 200);
}

/// Smoke test: a payload without signature headers must be rejected. Confirms the
/// signing-secret extractor is wired into the handler and the verification path
/// is reachable in the live app. All other rejection branches are unit-tested.
#[rstest]
#[tokio::test]
async fn test_slack_webhook_rejects_unsigned_request(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;

    let response = app
        .client
        .post(format!("{}hooks/slack/events", app.app.api_address))
        .json(&SlackPushEvent::UrlVerification(
            SlackUrlVerificationEvent {
                challenge: "test challenge".to_string(),
            },
        ))
        .send()
        .await
        .expect("Failed to execute request");

    assert_eq!(response.status(), 401);
}
