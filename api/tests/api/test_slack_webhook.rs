use chrono::{TimeZone, Utc};
use rstest::*;
use slack_morphism::prelude::{
    SlackAppHomeOpenedEvent, SlackAppRateLimitedEvent, SlackEventCallbackBody, SlackPushEvent,
    SlackPushEventCallback, SlackUrlVerificationEvent,
};

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::slack::SlackConfig,
    },
    notification::{NotificationMetadata, NotificationStatus},
};

use universal_inbox_api::{configuration::Settings, integrations::oauth2::NangoConnection};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{create_and_mock_integration_connection, nango_slack_connection},
    notification::{
        list_notifications,
        slack::{slack_push_star_added_event, slack_push_star_removed_event},
    },
    rest::create_resource_response,
    settings,
};

#[rstest]
#[tokio::test]
async fn test_receive_slack_url_verification_event(#[future] authenticated_app: AuthenticatedApp) {
    let app = authenticated_app.await;

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        Box::new(SlackPushEvent::UrlVerification(SlackUrlVerificationEvent {
            challenge: "test challenge".to_string(),
        })),
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

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        Box::new(SlackPushEvent::AppRateLimited(SlackAppRateLimitedEvent {
            team_id: "T123456".to_string(),
            minute_rate_limited: Utc::now().into(),
            api_app_id: "A123456".to_string(),
        })),
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

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        Box::new(SlackPushEvent::EventCallback(SlackPushEventCallback {
            team_id: "T123456".into(),
            api_app_id: "A123456".into(),
            event: SlackEventCallbackBody::AppHomeOpened(SlackAppHomeOpenedEvent {
                user: "U123456".into(),
                channel: "C123456".into(),
                tab: "home".into(),
                view: None,
            }),
            event_id: "Ev123456".into(),
            event_time: Utc::now().into(),
            event_context: None,
            authed_users: None,
            authorizations: None,
        })),
    )
    .await;

    // Return no error to Slack
    assert_eq!(response.status(), 200);
}

#[rstest]
#[tokio::test]
async fn test_receive_star_added_event_for_unknown_user(
    #[future] authenticated_app: AuthenticatedApp,
    slack_push_star_added_event: Box<SlackPushEvent>,
) {
    let app = authenticated_app.await;

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        slack_push_star_added_event.clone(),
    )
    .await;

    // Return no error to Slack
    assert_eq!(response.status(), 200);

    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
    )
    .await;
    assert!(notifications.is_empty());
}

#[rstest]
#[tokio::test]
async fn test_receive_star_added_event(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_push_star_added_event: Box<SlackPushEvent>,
    nango_slack_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled()),
        &settings,
        nango_slack_connection,
    )
    .await;

    let SlackPushEvent::EventCallback(event) = &*slack_push_star_added_event else {
        unreachable!("Unexpected event type");
    };

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        slack_push_star_added_event.clone(),
    )
    .await;

    assert_eq!(response.status(), 200);

    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].source_id, "1707686216.825719");
    assert_eq!(notifications[0].title, "🔴  *Test title* 🔴...");
    assert!(notifications[0].last_read_at.is_none());
    assert!(notifications[0].task_id.is_none());
    assert!(notifications[0].snoozed_until.is_none());
    assert_eq!(
        notifications[0].updated_at,
        Utc.with_ymd_and_hms(2024, 2, 12, 13, 9, 44).unwrap()
    );
    assert_eq!(
        notifications[0].metadata,
        NotificationMetadata::Slack(Box::new(event.clone()))
    );

    // A duplicated event should not create a new notification
    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        slack_push_star_added_event.clone(),
    )
    .await;
    assert_eq!(response.status(), 200);

    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].source_id, "1707686216.825719");
}

#[rstest]
#[tokio::test]
async fn test_receive_star_removed_event(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_push_star_added_event: Box<SlackPushEvent>,
    slack_push_star_removed_event: Box<SlackPushEvent>,
    nango_slack_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled()),
        &settings,
        nango_slack_connection,
    )
    .await;

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        slack_push_star_added_event.clone(),
    )
    .await;
    assert_eq!(response.status(), 200);

    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].source_id, "1707686216.825719");
    let star_added_notification_id = notifications[0].id;

    let SlackPushEvent::EventCallback(star_removed_event) = &*slack_push_star_removed_event else {
        unreachable!("Unexpected event type");
    };

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        slack_push_star_removed_event.clone(),
    )
    .await;
    assert_eq!(response.status(), 200);

    // No unread notification, it should have been deleted
    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
    )
    .await;
    assert_eq!(notifications.len(), 0);

    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Deleted],
        false,
        None,
        None,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].id, star_added_notification_id);
    assert_eq!(notifications[0].source_id, "1707686216.825719");
    assert_eq!(notifications[0].title, "🔴  *Test title* 🔴...");
    assert!(notifications[0].last_read_at.is_none());
    assert!(notifications[0].task_id.is_none());
    assert!(notifications[0].snoozed_until.is_none());
    assert_eq!(
        notifications[0].updated_at,
        Utc.with_ymd_and_hms(2024, 2, 12, 14, 15, 13).unwrap()
    );
    assert_eq!(
        notifications[0].metadata,
        NotificationMetadata::Slack(Box::new(star_removed_event.clone()))
    );
}
