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

mod slack_bridge {
    use chrono::Utc;
    use pretty_assertions::assert_eq;
    use slack_morphism::prelude::*;
    use uuid::Uuid;

    use universal_inbox::{
        integration_connection::integrations::slack::SlackConfig,
        notification::NotificationId,
        slack_bridge::{
            SlackBridgeActionStatus, SlackBridgeActionType, SlackBridgePendingAction,
            SlackBridgePendingActionId,
        },
        third_party::integrations::slack::SlackThread,
    };
    use universal_inbox_api::repository::slack_bridge::SlackBridgeRepository;

    use crate::helpers::{
        integration_connection::slack_oauth_credential,
        notification::slack::{create_notification_from_slack_thread, slack_thread},
    };

    use super::*;

    /// A Slack notification of a user whose Slack connection has the extension bridge enabled,
    /// or not.
    async fn create_slack_notification(
        app: &AuthenticatedApp,
        settings: &Settings,
        slack_oauth_credential: OAuthCredentialFixture,
        slack_thread: &SlackThread,
        extension_enabled: bool,
    ) -> NotificationId {
        let mut config = SlackConfig::enabled_as_notifications();
        config.message_config.extension_enabled = extension_enabled;
        let slack_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Slack(config),
            settings,
            slack_oauth_credential,
            None,
            None,
        )
        .await;

        create_notification_from_slack_thread(
            &app.app,
            slack_thread,
            app.user.id,
            slack_integration_connection.id,
        )
        .await
        .id
    }

    async fn delete_notification(app: &AuthenticatedApp, notification_id: NotificationId) {
        let response = patch_resource_response(
            &app.client,
            &app.app.api_address,
            "notifications",
            notification_id.into(),
            &NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    async fn pending_action_ids(app: &AuthenticatedApp) -> Vec<Uuid> {
        sqlx::query_scalar("SELECT id FROM slack_bridge_pending_action WHERE user_id = $1")
            .bind(app.user.id.0)
            .fetch_all(&*app.app.repository.pool)
            .await
            .unwrap()
    }

    /// An action stored without going through the service: it publishes no event.
    async fn seed_pending_action(app: &AuthenticatedApp) -> SlackBridgePendingActionId {
        let now = Utc::now();
        let action = SlackBridgePendingAction {
            id: Uuid::new_v4().into(),
            user_id: app.user.id,
            notification_id: None,
            action_type: SlackBridgeActionType::MarkAsRead,
            slack_team_id: SlackTeamId::new("T1234".to_string()),
            slack_channel_id: SlackChannelId::new("C1234".to_string()),
            slack_thread_ts: SlackTs::new("1700000000.000100".to_string()),
            slack_last_message_ts: SlackTs::new("1700000000.000200".to_string()),
            status: SlackBridgeActionStatus::Pending,
            failure_message: None,
            retry_count: 0,
            created_at: now,
            updated_at: now,
            completed_at: None,
        };
        let mut transaction = app.app.repository.begin().await.unwrap();
        let action = app
            .app
            .repository
            .create_pending_action(&mut transaction, &action)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        action.id
    }

    async fn post_action(app: &AuthenticatedApp, id: SlackBridgePendingActionId, action: &str) {
        let response = app
            .client
            .post(format!(
                "{}slack-bridge/actions/{}/{action}",
                app.app.api_address, id.0
            ))
            .json(&serde_json::json!({ "error": "failed" }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);
    }

    #[rstest]
    #[tokio::test]
    async fn test_delete_slack_notification_publishes_a_pending_actions_event(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        slack_oauth_credential: OAuthCredentialFixture,
        slack_thread: Box<SlackThread>,
    ) {
        let app = authenticated_app.await;
        let notification_id =
            create_slack_notification(&app, &settings, slack_oauth_credential, &slack_thread, true)
                .await;
        let mut subscriber = RealtimeEventsSubscriber::new(&settings, app.user.id).await;

        delete_notification(&app, notification_id).await;

        let events = subscriber
            .events_until(|events| {
                !events_of_kind(events, RealtimeEventKind::SlackBridgePendingActionsChanged)
                    .is_empty()
            })
            .await;
        let action_ids = pending_action_ids(&app).await;
        assert_eq!(action_ids.len(), 1);
        assert_eq!(
            events_of_kind(&events, RealtimeEventKind::SlackBridgePendingActionsChanged),
            vec![RealtimeEvent {
                user_id: app.user.id,
                kind: RealtimeEventKind::SlackBridgePendingActionsChanged,
                ids: Some(action_ids),
            }]
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_complete_or_fail_pending_action_publishes_no_pending_actions_event(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        slack_oauth_credential: OAuthCredentialFixture,
        slack_thread: Box<SlackThread>,
    ) {
        let app = authenticated_app.await;
        let notification_id =
            create_slack_notification(&app, &settings, slack_oauth_credential, &slack_thread, true)
                .await;
        let completed_action_id = seed_pending_action(&app).await;
        let failed_action_id = seed_pending_action(&app).await;
        let mut subscriber = RealtimeEventsSubscriber::new(&settings, app.user.id).await;

        post_action(&app, completed_action_id, "complete").await;
        post_action(&app, failed_action_id, "fail").await;
        // A later created action tells completing and failing published nothing before it
        delete_notification(&app, notification_id).await;

        let events = subscriber
            .events_until(|events| {
                !events_of_kind(events, RealtimeEventKind::SlackBridgePendingActionsChanged)
                    .is_empty()
            })
            .await;
        let created_action_ids: Vec<Uuid> = pending_action_ids(&app)
            .await
            .into_iter()
            .filter(|id| *id != completed_action_id.0 && *id != failed_action_id.0)
            .collect();
        assert_eq!(created_action_ids.len(), 1);
        assert_eq!(
            events_of_kind(&events, RealtimeEventKind::SlackBridgePendingActionsChanged),
            vec![RealtimeEvent {
                user_id: app.user.id,
                kind: RealtimeEventKind::SlackBridgePendingActionsChanged,
                ids: Some(created_action_ids),
            }]
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_delete_slack_notification_with_bridge_disabled_publishes_no_pending_actions_event(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        slack_oauth_credential: OAuthCredentialFixture,
        slack_thread: Box<SlackThread>,
    ) {
        let app = authenticated_app.await;
        let notification_id = create_slack_notification(
            &app,
            &settings,
            slack_oauth_credential,
            &slack_thread,
            false,
        )
        .await;
        let mut subscriber = RealtimeEventsSubscriber::new(&settings, app.user.id).await;

        delete_notification(&app, notification_id).await;

        // The deletion's own event closes the window a pending actions event would be in
        let events = subscriber
            .events_until(|events| {
                !events_of_kind(events, RealtimeEventKind::NotificationsChanged).is_empty()
            })
            .await;
        assert_eq!(
            events_of_kind(&events, RealtimeEventKind::SlackBridgePendingActionsChanged),
            vec![]
        );
        assert_eq!(pending_action_ids(&app).await, Vec::<Uuid>::new());
    }
}
