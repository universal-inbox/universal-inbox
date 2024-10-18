#![allow(clippy::too_many_arguments)]

use apalis::prelude::Storage;
use chrono::{TimeZone, Utc};
use pretty_assertions::assert_eq;
use rstest::*;
use slack_morphism::{
    events::SlackStarRemovedEvent,
    prelude::{
        SlackAppHomeOpenedEvent, SlackAppRateLimitedEvent, SlackEventCallbackBody, SlackPushEvent,
        SlackPushEventCallback, SlackStarAddedEvent, SlackStarsItem, SlackUrlVerificationEvent,
    },
    SlackReactionName,
};
use tokio::time::{sleep, Duration};

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{
            slack::{SlackConfig, SlackReactionConfig, SlackSyncType},
            todoist::TodoistConfig,
        },
    },
    notification::{NotificationDetails, NotificationMetadata, NotificationStatus},
    task::TaskStatus,
    third_party::{
        integrations::{
            slack::SlackMessageSenderDetails,
            todoist::{TodoistItem, TodoistItemPriority},
        },
        item::{ThirdPartyItemSource, ThirdPartyItemSourceKind},
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
            mock_slack_fetch_bot, mock_slack_fetch_channel, mock_slack_fetch_reply,
            mock_slack_fetch_team, mock_slack_fetch_user, mock_slack_get_chat_permalink,
            slack_push_bot_star_added_event, slack_push_reaction_added_event,
            slack_push_reaction_removed_event, slack_push_star_added_event,
            slack_push_star_removed_event,
        },
    },
    rest::create_resource_response,
    settings,
    task::{
        list_synced_tasks, list_tasks,
        todoist::{
            mock_todoist_complete_item_service, mock_todoist_get_item_service,
            mock_todoist_item_add_service, mock_todoist_sync_resources_service,
            mock_todoist_uncomplete_item_service, sync_todoist_projects_response, todoist_item,
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
#[case::star_added(true)]
#[case::reaction_added(false)]
#[tokio::test]
async fn test_receive_star_reaction_event_for_unknown_user(
    #[future] authenticated_app: AuthenticatedApp,
    slack_push_star_added_event: Box<SlackPushEvent>,
    slack_push_reaction_added_event: Box<SlackPushEvent>,
    #[case] star_added_case: bool,
) {
    let app = authenticated_app.await;

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        if star_added_case {
            slack_push_star_added_event
        } else {
            slack_push_reaction_added_event
        },
    )
    .await;

    assert_eq!(response.status(), 200);

    assert!(app
        .app
        .redis_storage
        .is_empty()
        .await
        .expect("Failed to get jobs count"));

    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
        false,
    )
    .await;
    assert!(notifications.is_empty());
}

#[rstest]
#[case::star_added(true)]
#[case::reaction_added(false)]
#[tokio::test]
async fn test_receive_star_reaction_event_for_disabled_config(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_push_star_added_event: Box<SlackPushEvent>,
    slack_push_reaction_added_event: Box<SlackPushEvent>,
    #[case] star_added_case: bool,
    nango_slack_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig::disabled()),
        &settings,
        nango_slack_connection,
        None,
    )
    .await;

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        if star_added_case {
            slack_push_star_added_event
        } else {
            slack_push_reaction_added_event
        },
    )
    .await;

    assert_eq!(response.status(), 200);

    assert!(app
        .app
        .redis_storage
        .is_empty()
        .await
        .expect("Failed to get jobs count"));

    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
        false,
    )
    .await;
    assert!(notifications.is_empty());
}

#[rstest]
#[tokio::test]
async fn test_receive_reaction_event_for_different_reaction(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_push_reaction_added_event: Box<SlackPushEvent>,
    nango_slack_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig {
            reaction_config: SlackReactionConfig {
                sync_enabled: true,
                reaction_name: SlackReactionName("bookmark".to_string()), // slack_push_reaction_added_event uses "eyes" reaction
                sync_type: SlackSyncType::AsNotifications,
            },
            ..SlackConfig::default()
        }),
        &settings,
        nango_slack_connection,
        None,
    )
    .await;

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        slack_push_reaction_added_event,
    )
    .await;

    assert_eq!(response.status(), 200);

    assert!(app
        .app
        .redis_storage
        .is_empty()
        .await
        .expect("Failed to get jobs count"));

    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
        false,
    )
    .await;
    assert!(notifications.is_empty());
}

#[rstest]
#[case::star_added(true)]
#[case::reaction_added(false)]
#[tokio::test]
async fn test_receive_star_or_reaction_added_event_as_notification(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    #[case] star_added_case: bool,
    slack_push_star_added_event: Box<SlackPushEvent>,
    slack_push_reaction_added_event: Box<SlackPushEvent>,
    nango_slack_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings,
        nango_slack_connection,
        None,
    )
    .await;

    // Not asserting this call as it could be cached by another test
    let _slack_get_chat_permalink_mock = mock_slack_get_chat_permalink(
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
    let slack_fetch_message_mock = mock_slack_fetch_reply(
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

    let SlackPushEvent::EventCallback(star_added_event) = &*slack_push_star_added_event else {
        unreachable!("Unexpected event type");
    };
    let SlackPushEvent::EventCallback(reaction_added_event) = &*slack_push_reaction_added_event
    else {
        unreachable!("Unexpected event type");
    };

    let response = if star_added_case {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_star_added_event.clone(),
        )
        .await
    } else {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_reaction_added_event.clone(),
        )
        .await
    };

    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);
    sleep(Duration::from_secs(1)).await;

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
        false,
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
    if star_added_case {
        assert_eq!(
            notifications[0].metadata,
            NotificationMetadata::Slack(Box::new(star_added_event.clone()))
        );
    } else {
        assert_eq!(
            notifications[0].metadata,
            NotificationMetadata::Slack(Box::new(reaction_added_event.clone()))
        );
    }
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

    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].source_id, "1707686216.825719");
}

#[rstest]
#[tokio::test]
async fn test_receive_bot_star_added_event_as_notification(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_push_bot_star_added_event: Box<SlackPushEvent>,
    nango_slack_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings,
        nango_slack_connection,
        None,
    )
    .await;

    // Not asserting this call as it could be cached by another test
    let _slack_get_chat_permalink_mock = mock_slack_get_chat_permalink(
        &app.app.slack_mock_server,
        "C05XXX",
        "1707686216.825719",
        "slack_get_chat_permalink_response.json",
    );
    let slack_fetch_bot_mock = mock_slack_fetch_bot(
        &app.app.slack_mock_server,
        "B05YYY", // The message's creator, not the user who starred the message
        "slack_fetch_bot_response.json",
    );
    let slack_fetch_message_mock = mock_slack_fetch_reply(
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

    let SlackPushEvent::EventCallback(event) = &*slack_push_bot_star_added_event else {
        unreachable!("Unexpected event type");
    };

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        slack_push_bot_star_added_event.clone(),
    )
    .await;

    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);
    sleep(Duration::from_secs(1)).await;

    slack_fetch_bot_mock.assert();
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
        false,
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
                SlackMessageSenderDetails::Bot(bot) => {
                    assert_eq!(bot.id, Some("B05YYY".into()));
                }
                _ => unreachable!("Expected a SlackMessageSenderDetails::Bot"),
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
        slack_push_bot_star_added_event.clone(),
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
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].source_id, "1707686216.825719");
}

#[rstest]
#[case::slack_star_without_thread(true, false)]
#[case::slack_star_with_message_in_thread(true, true)]
#[case::slack_reaction_without_thread(false, false)]
// no thread_ts in reaction: #[case::slack_reaction_with_message_in_thread(false, true)]
#[tokio::test]
async fn test_receive_star_or_reaction_removed_event_as_notification(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    mut slack_push_star_added_event: Box<SlackPushEvent>,
    slack_push_reaction_added_event: Box<SlackPushEvent>,
    mut slack_push_star_removed_event: Box<SlackPushEvent>,
    slack_push_reaction_removed_event: Box<SlackPushEvent>,
    nango_slack_connection: Box<NangoConnection>,
    #[case] slack_star_case: bool,
    #[case] with_message_in_thread: bool,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings,
        nango_slack_connection,
        None,
    )
    .await;

    // Not asserting this call as it could be cached by another test
    let _slack_get_chat_permalink_mock = mock_slack_get_chat_permalink(
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
    let slack_fetch_message_mock = if with_message_in_thread {
        let SlackPushEvent::EventCallback(SlackPushEventCallback {
            event:
                SlackEventCallbackBody::StarAdded(SlackStarAddedEvent {
                    item: SlackStarsItem::Message(ref mut added_message),
                    ..
                }),
            ..
        }) = *slack_push_star_added_event
        else {
            unreachable!("Unexpected event type");
        };
        let thread_ts = "1707686216.111111";
        added_message.message.origin.thread_ts = Some(thread_ts.into());

        let SlackPushEvent::EventCallback(SlackPushEventCallback {
            event:
                SlackEventCallbackBody::StarRemoved(SlackStarRemovedEvent {
                    item: SlackStarsItem::Message(ref mut removed_message),
                    ..
                }),
            ..
        }) = *slack_push_star_removed_event
        else {
            unreachable!("Unexpected event type");
        };
        removed_message.message.origin.thread_ts = Some(thread_ts.into());

        mock_slack_fetch_reply(
            &app.app.slack_mock_server,
            "C05XXX",
            thread_ts,
            "slack_fetch_message_response.json",
        )
    } else {
        mock_slack_fetch_reply(
            &app.app.slack_mock_server,
            "C05XXX",
            "1707686216.825719",
            "slack_fetch_message_response.json",
        )
    };
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

    let response = if slack_star_case {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_star_added_event.clone(),
        )
        .await
    } else {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_reaction_added_event.clone(),
        )
        .await
    };
    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);

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
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].source_id, "1707686216.825719");
    let star_added_notification_id = notifications[0].id;

    let SlackPushEvent::EventCallback(star_removed_event) = &*slack_push_star_removed_event else {
        unreachable!("Unexpected event type");
    };
    let SlackPushEvent::EventCallback(reaction_removed_event) = &*slack_push_reaction_removed_event
    else {
        unreachable!("Unexpected event type");
    };

    let response = if slack_star_case {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_star_removed_event.clone(),
        )
        .await
    } else {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_reaction_removed_event.clone(),
        )
        .await
    };
    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);
    sleep(Duration::from_secs(1)).await;

    // No unread notification, it should have been deleted
    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
        false,
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
        false,
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
    if slack_star_case {
        assert_eq!(
            notifications[0].metadata,
            NotificationMetadata::Slack(Box::new(star_removed_event.clone()))
        );
    } else {
        assert_eq!(
            notifications[0].metadata,
            NotificationMetadata::Slack(Box::new(reaction_removed_event.clone()))
        );
    }
}

#[rstest]
#[case::star_added(true)]
#[case::reaction_added(false)]
#[tokio::test]
async fn test_receive_star_or_reaction_added_event_as_task(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_push_star_added_event: Box<SlackPushEvent>,
    slack_push_reaction_added_event: Box<SlackPushEvent>,
    sync_todoist_projects_response: TodoistSyncResponse,
    todoist_item: Box<TodoistItem>,
    nango_slack_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
    #[case] slack_star_case: bool,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_tasks()),
        &settings,
        nango_slack_connection,
        None,
    )
    .await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
        None,
    )
    .await;

    // Not asserting this call as it could be cached by another test
    let _slack_get_chat_permalink_mock = mock_slack_get_chat_permalink(
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
    let slack_message_id = "1707686216.825719";
    let slack_fetch_message_mock = mock_slack_fetch_reply(
        &app.app.slack_mock_server,
        "C05XXX",
        slack_message_id,
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
        "[🔴  *Test title* 🔴...](https://slack.com/archives/C05XXX/p1234567890)".to_string(),
        Some(
            "🔴  *Test title* 🔴\n\n- list 1\n- list 2\n1. number 1\n1. number 2\n> quote\n```$ echo Hello world```\n_Some_ `formatted` ~text~.\n\nHere is a [link](https://www.universal-inbox.com)".to_string(),
        ),
        "1111".to_string(), // ie. "Inbox"
        None,
        TodoistItemPriority::P1,
    );
    let todoist_get_item_mock =
        mock_todoist_get_item_service(&app.app.todoist_mock_server, todoist_item.clone());

    let response = if slack_star_case {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_star_added_event.clone(),
        )
        .await
    } else {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_reaction_added_event.clone(),
        )
        .await
    };

    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);

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
        vec![],
        false,
        None,
        None,
        false,
    )
    .await;

    assert_eq!(notifications.len(), 0);

    let tasks = list_tasks(&app.client, &app.app.api_address, TaskStatus::Active, false).await;

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].status, TaskStatus::Active);
    let source_item = &tasks[0].source_item;
    assert_eq!(source_item.source_id, slack_message_id);
    assert_eq!(
        source_item.get_third_party_item_source_kind(),
        if slack_star_case {
            ThirdPartyItemSourceKind::SlackStar
        } else {
            ThirdPartyItemSourceKind::SlackReaction
        }
    );
    let sink_item = tasks[0].sink_item.as_ref().unwrap();
    assert_eq!(sink_item.source_id, todoist_item.id);
    assert_eq!(
        sink_item.get_third_party_item_source_kind(),
        ThirdPartyItemSourceKind::Todoist
    );

    let synced_tasks =
        list_synced_tasks(&app.client, &app.app.api_address, TaskStatus::Active, false).await;

    assert_eq!(synced_tasks.len(), 1);
    assert_eq!(synced_tasks[0].id, tasks[0].id);

    // A duplicated event should not create a new task
    let response = if slack_star_case {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_star_added_event.clone(),
        )
        .await
    } else {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_reaction_added_event.clone(),
        )
        .await
    };
    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);

    let tasks = list_tasks(&app.client, &app.app.api_address, TaskStatus::Active, false).await;

    assert_eq!(tasks.len(), 1);
}

#[rstest]
#[case::slack_star(true)]
#[case::slack_reaction(false)]
#[tokio::test]
async fn test_receive_star_or_reaction_removed_and_added_event_as_task(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_push_star_added_event: Box<SlackPushEvent>,
    slack_push_star_removed_event: Box<SlackPushEvent>,
    slack_push_reaction_added_event: Box<SlackPushEvent>,
    slack_push_reaction_removed_event: Box<SlackPushEvent>,
    sync_todoist_projects_response: TodoistSyncResponse,
    todoist_item: Box<TodoistItem>,
    nango_slack_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
    #[case] slack_star_case: bool,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_tasks()),
        &settings,
        nango_slack_connection,
        None,
    )
    .await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
        None,
    )
    .await;

    // Not asserting this call as it could be cached by another test
    let _slack_get_chat_permalink_mock = mock_slack_get_chat_permalink(
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
    let slack_message_id = "1707686216.825719";
    let slack_fetch_message_mock = mock_slack_fetch_reply(
        &app.app.slack_mock_server,
        "C05XXX",
        slack_message_id,
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
        "[🔴  *Test title* 🔴...](https://slack.com/archives/C05XXX/p1234567890)".to_string(),
        Some(
            "🔴  *Test title* 🔴\n\n- list 1\n- list 2\n1. number 1\n1. number 2\n> quote\n```$ echo Hello world```\n_Some_ `formatted` ~text~.\n\nHere is a [link](https://www.universal-inbox.com)".to_string(),
        ),
        "1111".to_string(), // ie. "Inbox"
        None,
        TodoistItemPriority::P1,
    );
    let todoist_get_item_mock =
        mock_todoist_get_item_service(&app.app.todoist_mock_server, todoist_item.clone());

    // First creation of a deleted notification and an active task
    let response = if slack_star_case {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_star_added_event.clone(),
        )
        .await
    } else {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_reaction_added_event.clone(),
        )
        .await
    };
    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);
    // Make sure the task will be updated
    sleep(Duration::from_secs(1)).await;

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
        false,
    )
    .await;

    assert_eq!(notifications.len(), 0);

    let tasks = list_tasks(&app.client, &app.app.api_address, TaskStatus::Active, false).await;

    assert_eq!(tasks.len(), 1);
    let star_added_task = &tasks[0];
    assert_eq!(tasks[0].status, TaskStatus::Active);
    let source_item = &tasks[0].source_item;
    assert_eq!(source_item.source_id, slack_message_id);
    assert_eq!(
        source_item.get_third_party_item_source_kind(),
        if slack_star_case {
            ThirdPartyItemSourceKind::SlackStar
        } else {
            ThirdPartyItemSourceKind::SlackReaction
        }
    );
    let sink_item = tasks[0].sink_item.as_ref().unwrap();
    assert_eq!(sink_item.source_id, todoist_item.id);
    assert_eq!(
        sink_item.get_third_party_item_source_kind(),
        ThirdPartyItemSourceKind::Todoist
    );

    let todoist_complete_item_mock =
        mock_todoist_complete_item_service(&app.app.todoist_mock_server, &sink_item.source_id);

    // Notification should still be marked as deleted and associated task as done
    let response = if slack_star_case {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_star_removed_event.clone(),
        )
        .await
    } else {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_reaction_removed_event.clone(),
        )
        .await
    };
    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);
    sleep(Duration::from_secs(1)).await;

    todoist_complete_item_mock.assert();

    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Deleted],
        false,
        None,
        None,
        false,
    )
    .await;

    assert_eq!(notifications.len(), 0);

    let tasks = list_tasks(&app.client, &app.app.api_address, TaskStatus::Done, false).await;

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, star_added_task.id);
    assert_eq!(tasks[0].status, TaskStatus::Done);
    let source_item = &tasks[0].source_item;
    assert_eq!(source_item.source_id, slack_message_id);
    assert_eq!(
        source_item.get_third_party_item_source_kind(),
        if slack_star_case {
            ThirdPartyItemSourceKind::SlackStar
        } else {
            ThirdPartyItemSourceKind::SlackReaction
        }
    );
    let sink_item = tasks[0].sink_item.as_ref().unwrap();
    assert_eq!(sink_item.source_id, todoist_item.id);
    assert_eq!(
        sink_item.get_third_party_item_source_kind(),
        ThirdPartyItemSourceKind::Todoist
    );

    let todoist_uncomplete_item_mock =
        mock_todoist_uncomplete_item_service(&app.app.todoist_mock_server, &sink_item.source_id);

    // Task should be marked as active again
    let response = if slack_star_case {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_star_added_event.clone(),
        )
        .await
    } else {
        create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_reaction_added_event.clone(),
        )
        .await
    };
    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);
    sleep(Duration::from_secs(1)).await;

    todoist_uncomplete_item_mock.assert();

    let tasks = list_tasks(&app.client, &app.app.api_address, TaskStatus::Active, false).await;

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, star_added_task.id);
    assert_eq!(tasks[0].status, TaskStatus::Active);
}
