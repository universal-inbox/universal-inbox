#![allow(clippy::too_many_arguments)]
use std::collections::HashMap;

use apalis::prelude::Storage;
use pretty_assertions::assert_eq;
use rstest::*;
use slack_blocks_render::SlackReferences;
use slack_morphism::prelude::*;
use tokio::time::{sleep, Duration};

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{
            slack::{SlackConfig, SlackReactionConfig, SlackSyncType},
            todoist::TodoistConfig,
        },
    },
    notification::{NotificationSourceKind, NotificationStatus},
    task::TaskStatus,
    third_party::{
        integrations::{
            slack::{SlackMessageSenderDetails, SlackReactionState, SlackStarItem, SlackStarState},
            todoist::{TodoistItem, TodoistItemPriority},
        },
        item::{ThirdPartyItemData, ThirdPartyItemSource, ThirdPartyItemSourceKind},
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
            mock_slack_list_usergroups, slack_push_bot_star_added_event,
            slack_push_reaction_added_event, slack_push_reaction_removed_event,
            slack_push_star_added_event, slack_push_star_removed_event,
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
    use universal_inbox::third_party::integrations::slack::SlackReactionItem;

    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings,
        nango_slack_connection,
        None,
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
    let slack_list_usergroups_mock = mock_slack_list_usergroups(
        &app.app.slack_mock_server,
        "slack_list_usergroups_response.json",
    );

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
    slack_list_usergroups_mock.assert();

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
    assert_eq!(notifications[0].source_item.source_id, "1707686216.825719");
    assert_eq!(notifications[0].title, "🔴  Test title 🔴...");
    assert_eq!(notifications[0].kind, NotificationSourceKind::Slack);
    assert!(notifications[0].last_read_at.is_none());
    assert!(notifications[0].task_id.is_none());
    assert!(notifications[0].snoozed_until.is_none());

    if star_added_case {
        let ThirdPartyItemData::SlackStar(slack_star) = &notifications[0].source_item.data else {
            unreachable!("Expected a SlackStar data");
        };
        assert_eq!(slack_star.state, SlackStarState::StarAdded);
        let SlackStarItem::SlackMessage(message) = &slack_star.item else {
            unreachable!("Expected a SlackMessage item");
        };
        assert_eq!(
            message.url,
            "https://slack.com/archives/C05XXX/p1234567890"
                .parse()
                .unwrap()
        );
        assert_eq!(message.message.origin.ts, "1707686216.825719".into());
        assert_eq!(message.channel.id, "C05XXX".into());
        assert_eq!(
            message.references,
            Some(SlackReferences {
                users: HashMap::from([(
                    SlackUserId("U05YYY".to_string()),
                    Some("john.doe".to_string()),
                )]),
                channels: HashMap::from([(
                    SlackChannelId("C05XXX".to_string()),
                    Some("test".to_string()),
                )]),
                usergroups: HashMap::from([(
                    SlackUserGroupId("S05ZZZ".to_string()),
                    Some("admins".to_string()),
                )]),
            })
        );
        match &message.sender {
            SlackMessageSenderDetails::User(user) => {
                assert_eq!(user.id, Some("U05YYY".into()));
            }
            _ => unreachable!("Expected a SlackMessageSenderDetails::User"),
        }
        assert_eq!(message.team.id, "T05XXX".into());
    } else {
        let ThirdPartyItemData::SlackReaction(slack_reaction) = &notifications[0].source_item.data
        else {
            unreachable!("Expected a SlackStar data");
        };
        let SlackReactionItem::SlackMessage(message) = &slack_reaction.item else {
            unreachable!("Expected a SlackMessage item");
        };
        assert_eq!(slack_reaction.state, SlackReactionState::ReactionAdded);
        assert_eq!(slack_reaction.name, "eyes".into());
        assert_eq!(
            message.url,
            "https://slack.com/archives/C05XXX/p1234567890"
                .parse()
                .unwrap()
        );
        assert_eq!(message.message.origin.ts, "1707686216.825719".into());
        assert_eq!(message.channel.id, "C05XXX".into());
        assert_eq!(
            message.references,
            Some(SlackReferences {
                users: HashMap::from([(
                    SlackUserId("U05YYY".to_string()),
                    Some("john.doe".to_string()),
                )]),
                channels: HashMap::from([(
                    SlackChannelId("C05XXX".to_string()),
                    Some("test".to_string()),
                )]),
                usergroups: HashMap::from([(
                    SlackUserGroupId("S05ZZZ".to_string()),
                    Some("admins".to_string()),
                )]),
            })
        );
        match &message.sender {
            SlackMessageSenderDetails::User(user) => {
                assert_eq!(user.id, Some("U05YYY".into()));
            }
            _ => unreachable!("Expected a SlackMessageSenderDetails::User"),
        }
        assert_eq!(message.team.id, "T05XXX".into());
    }
    assert_eq!(
        notifications[0].get_html_url(),
        "https://slack.com/archives/C05XXX/p1234567890"
            .parse()
            .unwrap()
    );

    // A duplicated event should not create a new notification
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
    assert_eq!(notifications[0].source_item.source_id, "1707686216.825719");
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
    let slack_fetch_user_mock = mock_slack_fetch_user(
        &app.app.slack_mock_server,
        "U05YYY", // The user ID found in the message
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
    let slack_list_usergroups_mock = mock_slack_list_usergroups(
        &app.app.slack_mock_server,
        "slack_list_usergroups_response.json",
    );

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
    slack_list_usergroups_mock.assert();
    slack_fetch_user_mock.assert();

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
    assert_eq!(notifications[0].source_item.source_id, "1707686216.825719");
    assert_eq!(notifications[0].title, "🔴  Test title 🔴...");
    assert_eq!(notifications[0].kind, NotificationSourceKind::Slack);
    assert!(notifications[0].last_read_at.is_none());
    assert!(notifications[0].task_id.is_none());
    assert!(notifications[0].snoozed_until.is_none());
    assert_eq!(
        notifications[0].get_html_url(),
        "https://slack.com/archives/C05XXX/p1234567890"
            .parse()
            .unwrap()
    );

    let ThirdPartyItemData::SlackStar(slack_star) = &notifications[0].source_item.data else {
        unreachable!("Expected a SlackStar data");
    };
    assert_eq!(slack_star.state, SlackStarState::StarAdded);
    let SlackStarItem::SlackMessage(message) = &slack_star.item else {
        unreachable!("Expected a SlackMessage item");
    };
    assert_eq!(
        message.url,
        "https://slack.com/archives/C05XXX/p1234567890"
            .parse()
            .unwrap()
    );
    assert_eq!(message.message.origin.ts, "1707686216.825719".into());
    assert_eq!(message.channel.id, "C05XXX".into());
    assert_eq!(
        message.references,
        Some(SlackReferences {
            users: HashMap::from([(
                SlackUserId("U05YYY".to_string()),
                Some("john.doe".to_string()),
            )]),
            channels: HashMap::from([(
                SlackChannelId("C05XXX".to_string()),
                Some("test".to_string()),
            )]),
            usergroups: HashMap::from([(
                SlackUserGroupId("S05ZZZ".to_string()),
                Some("admins".to_string()),
            )]),
        })
    );
    match &message.sender {
        SlackMessageSenderDetails::Bot(bot) => {
            assert_eq!(bot.id, Some("B05YYY".into()));
        }
        _ => unreachable!("Expected a SlackMessageSenderDetails::User"),
    }
    assert_eq!(message.team.id, "T05XXX".into());

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
    assert_eq!(notifications[0].source_item.source_id, "1707686216.825719");
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
    if with_message_in_thread {
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
    }
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
    let slack_list_usergroups_mock = mock_slack_list_usergroups(
        &app.app.slack_mock_server,
        "slack_list_usergroups_response.json",
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
    slack_list_usergroups_mock.assert();

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
    assert_eq!(notifications[0].source_item.source_id, "1707686216.825719");
    assert_eq!(notifications[0].kind, NotificationSourceKind::Slack);
    let star_added_notification_id = notifications[0].id;

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
    assert_eq!(notifications[0].source_item.source_id, "1707686216.825719");
    assert_eq!(notifications[0].title, "🔴  Test title 🔴...");
    assert_eq!(notifications[0].kind, NotificationSourceKind::Slack);
    assert!(notifications[0].last_read_at.is_none());
    assert!(notifications[0].task_id.is_none());
    assert!(notifications[0].snoozed_until.is_none());
    if slack_star_case {
        let ThirdPartyItemData::SlackStar(slack_star) = &notifications[0].source_item.data else {
            unreachable!("Expected a SlackStar data");
        };
        assert_eq!(slack_star.state, SlackStarState::StarRemoved);
    } else {
        let ThirdPartyItemData::SlackReaction(slack_reaction) = &notifications[0].source_item.data
        else {
            unreachable!("Expected a SlackReaction data");
        };
        assert_eq!(slack_reaction.state, SlackReactionState::ReactionRemoved);
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
    let slack_list_usergroups_mock = mock_slack_list_usergroups(
        &app.app.slack_mock_server,
        "slack_list_usergroups_response.json",
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
        "[🔴  Test title 🔴...](https://slack.com/archives/C05XXX/p1234567890)".to_string(),
        Some(
            "🔴  *Test title* 🔴\n\n- list 1\n- list 2\n1. number 1\n1. number 2\n> quote\n```$ echo Hello world```\n_Some_ `formatted` ~text~.\n\nHere is a [link](https://www.universal-inbox.com)@john.doe@admins#test".to_string(),
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
    slack_list_usergroups_mock.assert();
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
    let slack_list_usergroups_mock = mock_slack_list_usergroups(
        &app.app.slack_mock_server,
        "slack_list_usergroups_response.json",
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
        "[🔴  Test title 🔴...](https://slack.com/archives/C05XXX/p1234567890)".to_string(),
        Some(
            "🔴  *Test title* 🔴\n\n- list 1\n- list 2\n1. number 1\n1. number 2\n> quote\n```$ echo Hello world```\n_Some_ `formatted` ~text~.\n\nHere is a [link](https://www.universal-inbox.com)@john.doe@admins#test".to_string(),
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
    slack_list_usergroups_mock.assert();
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
