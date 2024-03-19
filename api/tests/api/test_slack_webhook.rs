use chrono::{TimeZone, Utc};
use rstest::*;
use slack_morphism::prelude::{
    SlackAppHomeOpenedEvent, SlackAppRateLimitedEvent, SlackEventCallbackBody, SlackPushEvent,
    SlackPushEventCallback, SlackUrlVerificationEvent,
};

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{slack::SlackConfig, todoist::TodoistConfig},
    },
    notification::{
        integrations::slack::SlackMessageSenderDetails, NotificationDetails, NotificationMetadata,
        NotificationStatus,
    },
    task::{
        integrations::todoist::{TodoistItem, TodoistItemPriority},
        TaskStatus,
    },
    HasHtmlUrl,
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{oauth2::NangoConnection, todoist::TodoistSyncResponse},
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_and_mock_integration_connection, nango_slack_connection, nango_todoist_connection,
    },
    job::wait_for_jobs_completion,
    notification::{
        list_notifications, list_notifications_with_tasks,
        slack::{
            mock_slack_fetch_channel, mock_slack_fetch_message, mock_slack_fetch_team,
            mock_slack_fetch_user, mock_slack_get_chat_permalink, slack_push_star_added_event,
            slack_push_star_removed_event,
        },
    },
    rest::create_resource_response,
    settings,
    task::{
        list_tasks,
        todoist::{
            mock_todoist_complete_item_service, mock_todoist_get_item_service,
            mock_todoist_item_add_service, mock_todoist_sync_resources_service,
            sync_todoist_projects_response, todoist_item,
        },
    },
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

    assert_eq!(response.status(), 200);

    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);

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
async fn test_receive_star_added_event_as_notification(
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
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings,
        nango_slack_connection,
    )
    .await;

    let slack_get_chat_permalink_mock = mock_slack_get_chat_permalink(
        &app.app.slack_mock_server,
        "C05XXX",
        "1707686216.825719",
        "slack_get_chat_permalink_response.json",
    );
    let slack_fetch_user_mock = mock_slack_fetch_user(
        &app.app.slack_mock_server,
        "U05YYY", // The message's creator, not the user who starred the message
        "slack_fetch_user_response.json",
    );
    let slack_fetch_message_mock = mock_slack_fetch_message(
        &app.app.slack_mock_server,
        "C05XXX",
        "1707686216.825719",
        "slack_fetch_message_response.json",
    );
    let slack_fetch_channel_mock = mock_slack_fetch_channel(
        &app.app.slack_mock_server,
        "C05XXX",
        "slack_fetch_channel_response.json",
    );
    let slack_fetch_team_mock = mock_slack_fetch_team(
        &app.app.slack_mock_server,
        "T05XXX",
        "slack_fetch_team_response.json",
    );

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
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);

    slack_get_chat_permalink_mock.assert();
    slack_fetch_user_mock.assert();
    slack_fetch_message_mock.assert();
    slack_fetch_channel_mock.assert();
    slack_fetch_team_mock.assert();

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
    assert_eq!(
        notifications[0].get_html_url(),
        "https://slack.com/archives/C05XXX/p1234567890"
            .parse()
            .unwrap()
    );

    match &notifications[0].details {
        Some(NotificationDetails::SlackMessage(details)) => {
            assert_eq!(
                details.url,
                "https://slack.com/archives/C05XXX/p1234567890"
                    .parse()
                    .unwrap()
            );
            assert_eq!(details.message.origin.ts, "1707686216.825719".into());
            assert_eq!(details.channel.id, "C05XXX".into());
            match &details.sender {
                SlackMessageSenderDetails::User(user) => {
                    assert_eq!(user.id, "U05YYY".into());
                }
                _ => unreachable!("Expected a SlackMessageSenderDetails::User"),
            }
            assert_eq!(details.team.id, "T05XXX".into());
        }
        _ => unreachable!("Expected a GithubDiscussion notification"),
    }

    // A duplicated event should not create a new notification
    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        slack_push_star_added_event.clone(),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);

    slack_get_chat_permalink_mock.assert_hits(1);
    slack_fetch_user_mock.assert_hits(1);
    slack_fetch_message_mock.assert_hits(1);
    slack_fetch_channel_mock.assert_hits(1);
    slack_fetch_team_mock.assert_hits(1);

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
async fn test_receive_star_removed_event_as_notification(
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
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings,
        nango_slack_connection,
    )
    .await;

    let slack_get_chat_permalink_mock = mock_slack_get_chat_permalink(
        &app.app.slack_mock_server,
        "C05XXX",
        "1707686216.825719",
        "slack_get_chat_permalink_response.json",
    );
    let slack_fetch_user_mock = mock_slack_fetch_user(
        &app.app.slack_mock_server,
        "U05YYY", // The message's creator, not the user who starred the message
        "slack_fetch_user_response.json",
    );
    let slack_fetch_message_mock = mock_slack_fetch_message(
        &app.app.slack_mock_server,
        "C05XXX",
        "1707686216.825719",
        "slack_fetch_message_response.json",
    );
    let slack_fetch_channel_mock = mock_slack_fetch_channel(
        &app.app.slack_mock_server,
        "C05XXX",
        "slack_fetch_channel_response.json",
    );
    let slack_fetch_team_mock = mock_slack_fetch_team(
        &app.app.slack_mock_server,
        "T05XXX",
        "slack_fetch_team_response.json",
    );

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        slack_push_star_added_event.clone(),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);

    slack_get_chat_permalink_mock.assert();
    slack_fetch_user_mock.assert();
    slack_fetch_message_mock.assert();
    slack_fetch_channel_mock.assert();
    slack_fetch_team_mock.assert();

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
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);

    slack_get_chat_permalink_mock.assert_hits(1);
    slack_fetch_user_mock.assert_hits(1);
    slack_fetch_message_mock.assert_hits(1);
    slack_fetch_channel_mock.assert_hits(1);
    slack_fetch_team_mock.assert_hits(1);

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

#[rstest]
#[tokio::test]
async fn test_receive_star_added_event_as_task(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_push_star_added_event: Box<SlackPushEvent>,
    sync_todoist_projects_response: TodoistSyncResponse,
    todoist_item: Box<TodoistItem>,
    nango_slack_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_tasks()),
        &settings,
        nango_slack_connection,
    )
    .await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
    )
    .await;

    let slack_get_chat_permalink_mock = mock_slack_get_chat_permalink(
        &app.app.slack_mock_server,
        "C05XXX",
        "1707686216.825719",
        "slack_get_chat_permalink_response.json",
    );
    let slack_fetch_user_mock = mock_slack_fetch_user(
        &app.app.slack_mock_server,
        "U05YYY", // The message's creator, not the user who starred the message
        "slack_fetch_user_response.json",
    );
    let slack_fetch_message_mock = mock_slack_fetch_message(
        &app.app.slack_mock_server,
        "C05XXX",
        "1707686216.825719",
        "slack_fetch_message_response.json",
    );
    let slack_fetch_channel_mock = mock_slack_fetch_channel(
        &app.app.slack_mock_server,
        "C05XXX",
        "slack_fetch_channel_response.json",
    );
    let slack_fetch_team_mock = mock_slack_fetch_team(
        &app.app.slack_mock_server,
        "T05XXX",
        "slack_fetch_team_response.json",
    );

    let todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    );
    let todoist_item_add_mock = mock_todoist_item_add_service(
        &app.app.todoist_mock_server,
        &todoist_item.id,
        "🔴  *Test title* 🔴...".to_string(),
        Some(
            "- [🔴  *Test title* 🔴...](https://slack.com/archives/C05XXX/p1234567890)".to_string(),
        ),
        "1111".to_string(), // ie. "Inbox"
        None,
        TodoistItemPriority::P1,
    );
    let todoist_get_item_mock =
        mock_todoist_get_item_service(&app.app.todoist_mock_server, todoist_item.clone());

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        slack_push_star_added_event.clone(),
    )
    .await;

    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);

    slack_get_chat_permalink_mock.assert();
    slack_fetch_user_mock.assert();
    slack_fetch_message_mock.assert();
    slack_fetch_channel_mock.assert();
    slack_fetch_team_mock.assert();
    todoist_projects_mock.assert();
    todoist_item_add_mock.assert();
    todoist_get_item_mock.assert();

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
    assert_eq!(notifications[0].source_id, "1707686216.825719");
    // See test `_as_notifications` for detailed assertions on the notification

    let tasks = list_tasks(&app.client, &app.app.api_address, TaskStatus::Active).await;

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].source_id, todoist_item.id);

    // A duplicated event should not create a new notification or new task
    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        slack_push_star_added_event.clone(),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);

    slack_get_chat_permalink_mock.assert_hits(1);
    slack_fetch_user_mock.assert_hits(1);
    slack_fetch_message_mock.assert_hits(1);
    slack_fetch_channel_mock.assert_hits(1);
    slack_fetch_team_mock.assert_hits(1);
    todoist_projects_mock.assert_hits(1);

    let notifications = list_notifications_with_tasks(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Deleted],
        false,
        None,
        None,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].source_id, "1707686216.825719");
    assert!(notifications[0].task.is_some());
    assert_eq!(
        notifications[0].task.as_ref().unwrap().source_id,
        todoist_item.id
    );

    let tasks = list_tasks(&app.client, &app.app.api_address, TaskStatus::Active).await;

    assert_eq!(tasks.len(), 1);
    assert_eq!(notifications[0].task.as_ref().unwrap().id, tasks[0].id);
}

#[rstest]
#[tokio::test]
#[allow(clippy::too_many_arguments)]
async fn test_receive_star_removed_event_as_task(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_push_star_added_event: Box<SlackPushEvent>,
    slack_push_star_removed_event: Box<SlackPushEvent>,
    sync_todoist_projects_response: TodoistSyncResponse,
    todoist_item: Box<TodoistItem>,
    nango_slack_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_tasks()),
        &settings,
        nango_slack_connection,
    )
    .await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
    )
    .await;

    let slack_get_chat_permalink_mock = mock_slack_get_chat_permalink(
        &app.app.slack_mock_server,
        "C05XXX",
        "1707686216.825719",
        "slack_get_chat_permalink_response.json",
    );
    let slack_fetch_user_mock = mock_slack_fetch_user(
        &app.app.slack_mock_server,
        "U05YYY", // The message's creator, not the user who starred the message
        "slack_fetch_user_response.json",
    );
    let slack_fetch_message_mock = mock_slack_fetch_message(
        &app.app.slack_mock_server,
        "C05XXX",
        "1707686216.825719",
        "slack_fetch_message_response.json",
    );
    let slack_fetch_channel_mock = mock_slack_fetch_channel(
        &app.app.slack_mock_server,
        "C05XXX",
        "slack_fetch_channel_response.json",
    );
    let slack_fetch_team_mock = mock_slack_fetch_team(
        &app.app.slack_mock_server,
        "T05XXX",
        "slack_fetch_team_response.json",
    );

    let todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    );
    let todoist_item_add_mock = mock_todoist_item_add_service(
        &app.app.todoist_mock_server,
        &todoist_item.id,
        "🔴  *Test title* 🔴...".to_string(),
        Some(
            "- [🔴  *Test title* 🔴...](https://slack.com/archives/C05XXX/p1234567890)".to_string(),
        ),
        "1111".to_string(), // ie. "Inbox"
        None,
        TodoistItemPriority::P1,
    );
    let todoist_get_item_mock =
        mock_todoist_get_item_service(&app.app.todoist_mock_server, todoist_item.clone());

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        slack_push_star_added_event.clone(),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);

    slack_get_chat_permalink_mock.assert();
    slack_fetch_user_mock.assert();
    slack_fetch_message_mock.assert();
    slack_fetch_channel_mock.assert();
    slack_fetch_team_mock.assert();
    todoist_projects_mock.assert();
    todoist_item_add_mock.assert();
    todoist_get_item_mock.assert();

    let notifications = list_notifications_with_tasks(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Deleted],
        false,
        None,
        None,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].source_id, "1707686216.825719");
    let star_added_notification = &notifications[0];
    let star_added_task = notifications[0].task.as_ref().unwrap();

    let todoist_complete_item_mock = mock_todoist_complete_item_service(
        &app.app.todoist_mock_server,
        &star_added_task.source_id,
    );

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        slack_push_star_removed_event.clone(),
    )
    .await;
    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);

    slack_get_chat_permalink_mock.assert_hits(1);
    slack_fetch_user_mock.assert_hits(1);
    slack_fetch_message_mock.assert_hits(1);
    slack_fetch_channel_mock.assert_hits(1);
    slack_fetch_team_mock.assert_hits(1);
    todoist_complete_item_mock.assert();

    // notification should still be marked as deleted
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
    assert_eq!(notifications[0].id, star_added_notification.id);

    let tasks = list_tasks(&app.client, &app.app.api_address, TaskStatus::Done).await;

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, star_added_task.id);
}
