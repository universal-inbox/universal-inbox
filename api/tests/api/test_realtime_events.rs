use std::time::Duration;

use futures::StreamExt;
use graphql_client::Response;
use pretty_assertions::assert_eq;
use rstest::*;
use tokio::time::{Instant, timeout_at};

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::github::GithubConfig,
    },
    notification::{NotificationSourceKind, NotificationStatus, service::NotificationPatch},
    realtime::{RealtimeEvent, RealtimeEventKind},
    third_party::integrations::github::GithubNotification,
    user::UserId,
};

use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use universal_inbox_api::{
    configuration::{RealtimeSettings, Settings},
    integrations::github::graphql::pull_request_query,
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        OAuthCredentialFixture, create_and_mock_integration_connection, github_oauth_credential,
    },
    notification::{
        github::{
            create_notification_from_github_notification, github_notification,
            github_pull_request_123_response, mock_github_notifications_service,
            mock_github_pull_request_query, sync_github_notifications,
        },
        sync_notifications,
    },
    rest::patch_resource_response,
    settings,
};

/// Subscriber to the realtime events channel, keeping only the events of one user: tests
/// running in parallel publish on the same channel.
struct RealtimeEventsSubscriber {
    pubsub: redis::aio::PubSub,
    user_id: UserId,
}

impl RealtimeEventsSubscriber {
    async fn new(settings: &Settings, user_id: UserId) -> Self {
        let client = redis::Client::open(settings.redis.connection_string()).unwrap();
        let mut pubsub = client.get_async_pubsub().await.unwrap();
        pubsub
            .subscribe(RealtimeSettings::default().channel)
            .await
            .unwrap();
        Self { pubsub, user_id }
    }

    /// The events of the user received until `until` returns true for them, then for a short
    /// while (300 ms) to catch unexpected extra events. An extra event arriving later than that
    /// goes unnoticed: under load the test can miss one, it cannot fail spuriously.
    async fn events_until(
        &mut self,
        until: impl Fn(&[RealtimeEvent]) -> bool,
    ) -> Vec<RealtimeEvent> {
        let user_id = self.user_id;
        let mut messages = self.pubsub.on_message();
        let mut events = vec![];
        let mut deadline = Instant::now() + Duration::from_secs(10);
        let mut done = false;
        while let Ok(Some(message)) = timeout_at(deadline, messages.next()).await {
            let payload: String = message.get_payload().unwrap();
            let event: RealtimeEvent = serde_json::from_str(&payload).unwrap();
            if event.user_id != user_id {
                continue;
            }
            events.push(event);
            if !done && until(&events) {
                done = true;
                deadline = Instant::now() + Duration::from_millis(300);
            }
        }
        assert!(
            done,
            "Expected realtime events not received, got {events:?}"
        );
        events
    }
}

fn events_of_kind(events: &[RealtimeEvent], kind: RealtimeEventKind) -> Vec<RealtimeEvent> {
    events
        .iter()
        .filter(|event| event.kind == kind)
        .cloned()
        .collect()
}

#[rstest]
#[tokio::test]
async fn test_patch_notification_publishes_a_realtime_event(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    github_notification: Box<GithubNotification>,
    github_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let github_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;
    let notification = create_notification_from_github_notification(
        &app.app,
        &github_notification,
        app.user.id,
        github_integration_connection.id,
    )
    .await;
    let mut subscriber = RealtimeEventsSubscriber::new(&settings, app.user.id).await;

    let response = patch_resource_response(
        &app.client,
        &app.app.api_address,
        "notifications",
        notification.id.into(),
        &NotificationPatch {
            snoozed_until: Some(chrono::Utc::now() + chrono::TimeDelta::hours(1)),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(response.status(), http::StatusCode::OK);

    let events = subscriber
        .events_until(|events| {
            !events_of_kind(events, RealtimeEventKind::NotificationsChanged).is_empty()
        })
        .await;
    assert_eq!(
        events_of_kind(&events, RealtimeEventKind::NotificationsChanged),
        vec![RealtimeEvent {
            user_id: app.user.id,
            kind: RealtimeEventKind::NotificationsChanged,
            ids: Some(vec![notification.id.0]),
        }]
    );
}

#[rstest]
#[tokio::test]
async fn test_patch_unknown_notification_publishes_nothing(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    github_notification: Box<GithubNotification>,
    github_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let github_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;
    let notification = create_notification_from_github_notification(
        &app.app,
        &github_notification,
        app.user.id,
        github_integration_connection.id,
    )
    .await;
    let mut subscriber = RealtimeEventsSubscriber::new(&settings, app.user.id).await;

    // Patching an unknown notification fails and changes nothing
    let response = patch_resource_response(
        &app.client,
        &app.app.api_address,
        "notifications",
        uuid::Uuid::new_v4(),
        &NotificationPatch {
            snoozed_until: Some(chrono::Utc::now() + chrono::TimeDelta::hours(1)),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
    // A later successful patch tells the failed one published nothing before it
    patch_resource_response(
        &app.client,
        &app.app.api_address,
        "notifications",
        notification.id.into(),
        &NotificationPatch {
            snoozed_until: Some(chrono::Utc::now() + chrono::TimeDelta::hours(1)),
            ..Default::default()
        },
    )
    .await;

    let events = subscriber
        .events_until(|events| {
            !events_of_kind(events, RealtimeEventKind::NotificationsChanged).is_empty()
        })
        .await;
    assert_eq!(
        events_of_kind(&events, RealtimeEventKind::NotificationsChanged),
        vec![RealtimeEvent {
            user_id: app.user.id,
            kind: RealtimeEventKind::NotificationsChanged,
            ids: Some(vec![notification.id.0]),
        }]
    );
}

#[rstest]
#[tokio::test]
async fn test_rolled_back_patch_notification_publishes_nothing(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    github_notification: Box<GithubNotification>,
    github_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let github_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;
    let notification = create_notification_from_github_notification(
        &app.app,
        &github_notification,
        app.user.id,
        github_integration_connection.id,
    )
    .await;
    // Deleting the notification on GitHub fails once the notification is updated: the update is
    // rolled back
    Mock::given(method("DELETE"))
        .and(path("/notifications/threads/1"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&app.app.github_mock_server)
        .await;
    let mut subscriber = RealtimeEventsSubscriber::new(&settings, app.user.id).await;

    let response = patch_resource_response(
        &app.client,
        &app.app.api_address,
        "notifications",
        notification.id.into(),
        &NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            ..Default::default()
        },
    )
    .await;
    assert!(!response.status().is_success());
    // A later successful patch shows the rolled back one published nothing before it
    let response = patch_resource_response(
        &app.client,
        &app.app.api_address,
        "notifications",
        notification.id.into(),
        &NotificationPatch {
            snoozed_until: Some(chrono::Utc::now() + chrono::TimeDelta::hours(1)),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(response.status(), http::StatusCode::OK);

    let events = subscriber
        .events_until(|events| {
            !events_of_kind(events, RealtimeEventKind::NotificationsChanged).is_empty()
        })
        .await;
    assert_eq!(
        events_of_kind(&events, RealtimeEventKind::NotificationsChanged),
        vec![RealtimeEvent {
            user_id: app.user.id,
            kind: RealtimeEventKind::NotificationsChanged,
            ids: Some(vec![notification.id.0]),
        }]
    );
}

#[rstest]
#[tokio::test]
async fn test_sync_notifications_publishes_one_batched_realtime_event(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    // Vec[GithubNotification { source_id: "123", ... }, GithubNotification { source_id: "456", ... } ]
    sync_github_notifications: Vec<GithubNotification>,
    github_pull_request_123_response: Response<pull_request_query::ResponseData>,
    github_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let github_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings,
        github_oauth_credential,
        None,
        None,
    )
    .await;
    let _github_notifications_mock = mock_github_notifications_service(
        &app.app.github_mock_server,
        "1",
        &sync_github_notifications,
    )
    .await;
    let empty_result = Vec::<GithubNotification>::new();
    let _github_notifications_mock2 =
        mock_github_notifications_service(&app.app.github_mock_server, "2", &empty_result).await;
    let _github_pull_request_123_query_mock = mock_github_pull_request_query(
        &app.app.github_mock_server,
        "octokit".to_string(),
        "octokit.rb".to_string(),
        123,
        &github_pull_request_123_response,
    )
    .await;
    let mut subscriber = RealtimeEventsSubscriber::new(&settings, app.user.id).await;

    let mut notifications = sync_notifications(
        &app.client,
        &app.app.api_address,
        Some(NotificationSourceKind::Github),
        false,
    )
    .await;
    assert_eq!(notifications.len(), sync_github_notifications.len());

    // The sync is claimed, then completed: the connection changes twice
    let events = subscriber
        .events_until(|events| {
            events_of_kind(events, RealtimeEventKind::IntegrationConnectionChanged).len() == 2
        })
        .await;
    notifications.sort_by_key(|notification| notification.id.0);
    assert_eq!(
        events_of_kind(&events, RealtimeEventKind::NotificationsChanged),
        vec![RealtimeEvent {
            user_id: app.user.id,
            kind: RealtimeEventKind::NotificationsChanged,
            ids: Some(
                notifications
                    .iter()
                    .map(|notification| notification.id.0)
                    .collect()
            ),
        }]
    );
    assert_eq!(
        events_of_kind(&events, RealtimeEventKind::IntegrationConnectionChanged),
        vec![
            RealtimeEvent {
                user_id: app.user.id,
                kind: RealtimeEventKind::IntegrationConnectionChanged,
                ids: Some(vec![github_integration_connection.id.0]),
            };
            2
        ]
    );
}
