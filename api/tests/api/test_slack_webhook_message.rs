#![allow(clippy::too_many_arguments)]
use anyhow::Context;
use apalis::prelude::*;
use rstest::*;
use serde_json::json;
use slack_morphism::prelude::*;

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionStatus, config::IntegrationConnectionConfig,
        integrations::slack::SlackConfig, integrations::slack::SlackContext,
        provider::IntegrationConnectionContext,
    },
    notification::{NotificationSourceKind, NotificationStatus},
    third_party::{integrations::slack::SlackThread, item::ThirdPartyItemData},
};

use universal_inbox_api::{
    configuration::Settings, integrations::oauth2::NangoConnection,
    jobs::slack::slack_message::handle_slack_message_push_event,
};

use crate::helpers::{
    TestedApp,
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, create_integration_connection,
        nango_slack_connection,
    },
    notification::{
        list_notifications,
        slack::{
            create_notification_from_slack_thread, mock_slack_fetch_channel, mock_slack_fetch_team,
            mock_slack_fetch_thread, mock_slack_fetch_user, mock_slack_get_chat_permalink,
            mock_slack_list_users_in_usergroup, slack_push_message_event,
            slack_push_message_in_thread_event, slack_thread,
        },
    },
    rest::create_resource_response,
    settings, tested_app_with_local_auth,
    user::create_user_and_login,
};

mod webhook {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_receive_slack_message_in_channel_without_ping_to_known_users(
        #[future] authenticated_app: AuthenticatedApp,
        slack_push_message_event: Box<SlackPushEvent>,
    ) {
        let mut app = authenticated_app.await;

        let response = create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_message_event.clone(),
        )
        .await;

        assert_eq!(response.status(), 200);
        assert_message_ignored(&mut app.app).await;
    }

    #[rstest]
    #[tokio::test]
    async fn test_receive_slack_message_in_channel_with_references(
        #[future] authenticated_app: AuthenticatedApp,
        mut slack_push_message_event: Box<SlackPushEvent>,
    ) {
        let mut app = authenticated_app.await;
        let SlackPushEvent::EventCallback(event) = &mut *slack_push_message_event else {
            unreachable!("Unexpected event type");
        };
        add_user_ref_in_message(event, "user1");

        let response = create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_message_event.clone(),
        )
        .await;

        assert_eq!(response.status(), 200);
        assert_message_processed(&mut app.app).await;
    }

    #[rstest]
    #[tokio::test]
    async fn test_receive_slack_message_in_an_unknown_thread(
        #[future] authenticated_app: AuthenticatedApp,
        slack_push_message_in_thread_event: Box<SlackPushEvent>,
    ) {
        let mut app = authenticated_app.await;

        let response = create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_message_in_thread_event.clone(),
        )
        .await;

        assert_eq!(response.status(), 200);
        assert_message_ignored(&mut app.app).await;
    }

    #[rstest]
    #[case::subscribed(true)]
    #[case::unsubscribed(false)]
    #[tokio::test]
    async fn test_receive_slack_message_in_known_thread(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_slack_connection: Box<NangoConnection>,
        mut slack_push_message_in_thread_event: Box<SlackPushEvent>,
        mut slack_thread: Box<SlackThread>,
        #[case] subscribed: bool,
    ) {
        let mut app = authenticated_app.await;
        let slack_integration_connection = create_and_mock_integration_connection(
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

        let SlackPushEvent::EventCallback(SlackPushEventCallback {
            event: SlackEventCallbackBody::Message(ref mut message),
            ..
        }) = *slack_push_message_in_thread_event
        else {
            unreachable!("Unexpected event type");
        };
        message.origin.thread_ts = Some(slack_thread.messages.first().origin.ts.clone());
        slack_thread.subscribed = subscribed;

        create_notification_from_slack_thread(
            &app.app,
            &slack_thread,
            app.user.id,
            slack_integration_connection.id,
        )
        .await;

        let response = create_resource_response(
            &app.client,
            &app.app.api_address,
            "hooks/slack/events",
            slack_push_message_in_thread_event.clone(),
        )
        .await;

        assert_eq!(response.status(), 200);
        // Message is processed even if thread is marked as unsubscribed as it may
        // have been marked as subscribed again from Slack. Universal Inbox must
        // update the thread to get the actual subscription status.
        assert_message_processed(&mut app.app).await;
    }

    #[rstest]
    #[tokio::test]
    async fn test_receive_slack_message_in_known_thread_from_known_user(
        settings: Settings,
        #[future] tested_app_with_local_auth: TestedApp,
        mut nango_slack_connection: Box<NangoConnection>,
        mut slack_push_message_in_thread_event: Box<SlackPushEvent>,
        mut slack_thread: Box<SlackThread>,
    ) {
        let mut app = tested_app_with_local_auth.await;
        // `slack_thread` contains 2 messages from 2 different users:
        // - first (read) message from user U01
        // - second (unread) message from user U02 (this message is also the one that is received in the event)
        nango_slack_connection.credentials.raw = json !({
            "authed_user": { "id": "U02", "access_token": "slack_test_user_access_token" },
            "team": { "id": "T01" }
        });
        let (client, user) =
            create_user_and_login(&app, "john@doe.net".parse().unwrap(), "password").await;

        let slack_integration_connection = create_and_mock_integration_connection(
            &app,
            user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            &settings,
            nango_slack_connection.clone(),
            None,
            None,
        )
        .await;

        let SlackPushEvent::EventCallback(SlackPushEventCallback {
            event: SlackEventCallbackBody::Message(ref mut message),
            ..
        }) = *slack_push_message_in_thread_event
        else {
            unreachable!("Unexpected event type");
        };
        message.origin.thread_ts = Some(slack_thread.messages.first().origin.ts.clone());
        slack_thread.subscribed = true;

        create_notification_from_slack_thread(
            &app,
            &slack_thread,
            user.id,
            slack_integration_connection.id,
        )
        .await;

        let response = create_resource_response(
            &client,
            &app.api_address,
            "hooks/slack/events",
            slack_push_message_in_thread_event.clone(),
        )
        .await;

        assert_eq!(response.status(), 200);
        assert_message_processed(&mut app).await;
    }

    async fn assert_message_ignored(app: &mut TestedApp) {
        assert!(
            app.redis_storage
                .is_empty()
                .await
                .expect("Failed to get jobs count")
        );
    }

    async fn assert_message_processed(app: &mut TestedApp) {
        assert!(
            !app.redis_storage
                .is_empty()
                .await
                .expect("Failed to get jobs count")
        );
    }
}

mod job {
    use crate::helpers::notification::slack::mock_slack_list_emojis;

    use super::*;
    use pretty_assertions::assert_eq;

    #[fixture]
    fn message_event(slack_push_message_event: Box<SlackPushEvent>) -> Box<SlackPushEventCallback> {
        match *slack_push_message_event {
            SlackPushEvent::EventCallback(event) => Box::new(event),
            _ => unreachable!("Unexpected event type"),
        }
    }

    #[fixture]
    fn message_in_thread_event(
        slack_push_message_in_thread_event: Box<SlackPushEvent>,
    ) -> Box<SlackPushEventCallback> {
        match *slack_push_message_in_thread_event {
            SlackPushEvent::EventCallback(event) => Box::new(event),
            _ => unreachable!("Unexpected event type"),
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_handle_slack_message_in_channel_without_ping(
        #[future] authenticated_app: AuthenticatedApp,
        message_event: Box<SlackPushEventCallback>,
    ) {
        let app = authenticated_app.await;
        let service = app.app.notification_service.read().await;
        let mut transaction = service.begin().await.unwrap();

        handle_slack_message_push_event(
            &mut transaction,
            &message_event,
            app.app.notification_service.clone(),
            app.app.integration_connection_service.clone(),
            app.app.third_party_item_service.clone(),
            app.app.slack_service.clone(),
        )
        .await
        .unwrap();

        transaction
            .commit()
            .await
            .context("Failed to commit transaction")
            .unwrap();

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
        assert!(notifications.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_handle_slack_message_in_channel_with_disabled_sync(
        #[future] authenticated_app: AuthenticatedApp,
        mut message_event: Box<SlackPushEventCallback>,
    ) {
        let app = authenticated_app.await;
        let service = app.app.notification_service.read().await;
        let mut transaction = service.begin().await.unwrap();
        create_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Slack(SlackConfig::disabled()),
            IntegrationConnectionStatus::Validated,
            None,
            Some("U01".to_string()),
            None,
            None,
        )
        .await;
        add_user_ref_in_message(&mut message_event, "U01");

        handle_slack_message_push_event(
            &mut transaction,
            &message_event,
            app.app.notification_service.clone(),
            app.app.integration_connection_service.clone(),
            app.app.third_party_item_service.clone(),
            app.app.slack_service.clone(),
        )
        .await
        .unwrap();

        transaction
            .commit()
            .await
            .context("Failed to commit transaction")
            .unwrap();

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
        assert!(notifications.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_handle_slack_message_in_channel_with_ping_to_unknown_user(
        #[future] authenticated_app: AuthenticatedApp,
        mut message_event: Box<SlackPushEventCallback>,
    ) {
        let app = authenticated_app.await;
        let service = app.app.notification_service.read().await;
        let mut transaction = service.begin().await.unwrap();
        create_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            IntegrationConnectionStatus::Validated,
            None,
            Some("U01".to_string()),
            None,
            None,
        )
        .await;
        add_user_ref_in_message(&mut message_event, "U02");

        handle_slack_message_push_event(
            &mut transaction,
            &message_event,
            app.app.notification_service.clone(),
            app.app.integration_connection_service.clone(),
            app.app.third_party_item_service.clone(),
            app.app.slack_service.clone(),
        )
        .await
        .unwrap();

        transaction
            .commit()
            .await
            .context("Failed to commit transaction")
            .unwrap();

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
        assert!(notifications.is_empty());
    }

    #[rstest]
    #[tokio::test]
    async fn test_handle_slack_message_in_channel_with_ping_to_known_user_from_known_user(
        settings: Settings,
        #[future] tested_app_with_local_auth: TestedApp,
        mut nango_slack_connection: Box<NangoConnection>,
        mut message_event: Box<SlackPushEventCallback>,
    ) {
        // Group `G01` contains user `U01` and user `U02`
        // The message received is sent by `U02` thus a notification should be created
        // only for `U01`.
        // Nonetheless, a ThirdPartyItem is still created for `U02` as it is the sender.
        let app = tested_app_with_local_auth.await;
        let service = app.notification_service.read().await;
        let mut transaction = service.begin().await.unwrap();
        add_usergroup_ref_in_message(&mut message_event, "G01");
        let slack_list_users_in_usergroup_mock = mock_slack_list_users_in_usergroup(
            &app.slack_mock_server,
            "G01",
            "slack_list_users_in_usergroup_response.json",
        );

        nango_slack_connection.credentials.raw["authed_user"]["id"] = json!("U02");
        nango_slack_connection.credentials.raw["authed_user"]["access_token"] =
            json!("slack_other_user_access_token");
        nango_slack_connection.credentials.raw["team"]["id"] = json!("T01");
        let (client_u02, user_u02) =
            create_user_and_login(&app, "john@doe.net".parse().unwrap(), "password").await;
        create_and_mock_integration_connection(
            &app,
            user_u02.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            &settings,
            nango_slack_connection.clone(),
            None,
            Some(IntegrationConnectionContext::Slack(SlackContext {
                team_id: SlackTeamId("T01".to_string()),
            })),
        )
        .await;

        nango_slack_connection.credentials.raw["authed_user"]["id"] = json!("U01");
        nango_slack_connection.credentials.raw["authed_user"]["access_token"] =
            json!("slack_test_user_access_token");
        nango_slack_connection.credentials.raw["team"]["id"] = json!("T01");
        let (client_u01, user_u01) =
            create_user_and_login(&app, "jane@doe.net".parse().unwrap(), "password").await;
        create_and_mock_integration_connection(
            &app,
            user_u01.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            &settings,
            nango_slack_connection,
            None,
            Some(IntegrationConnectionContext::Slack(SlackContext {
                team_id: SlackTeamId("T01".to_string()),
            })),
        )
        .await;

        let slack_fetch_user_mock1 = mock_slack_fetch_user(
            &app.slack_mock_server,
            "U01",
            "slack_fetch_user_response.json",
        );
        let slack_fetch_user_mock2 = mock_slack_fetch_user(
            &app.slack_mock_server,
            "U02",
            "slack_fetch_user_response.json",
        );

        let slack_message_id = "1732535291.911209"; // First message
        let slack_fetch_thread_mock_u01 = mock_slack_fetch_thread(
            &app.slack_mock_server,
            "C05XXX",
            slack_message_id,
            slack_message_id,
            "slack_fetch_thread_response.json",
            true,
            None,
            "slack_test_user_access_token",
        );
        mock_slack_get_chat_permalink(
            &app.slack_mock_server,
            "C05XXX",
            slack_message_id,
            "slack_get_chat_permalink_response.json",
        );
        mock_slack_list_emojis(&app.slack_mock_server, "slack_emoji_list_response.json");

        let slack_fetch_thread_mock_u02 = mock_slack_fetch_thread(
            &app.slack_mock_server,
            "C05XXX",
            slack_message_id,
            slack_message_id,
            "slack_fetch_thread_response.json",
            true,
            Some(1),
            "slack_other_user_access_token",
        );
        mock_slack_get_chat_permalink(
            &app.slack_mock_server,
            "C05XXX",
            "1729779674.478289",
            "slack_get_chat_permalink_response.json",
        );

        let slack_fetch_channel_mock = mock_slack_fetch_channel(
            &app.slack_mock_server,
            "C05XXX",
            "slack_fetch_channel_response.json",
        );
        let slack_fetch_team_mock = mock_slack_fetch_team(
            &app.slack_mock_server,
            "T01",
            "slack_fetch_team_response.json",
        );

        handle_slack_message_push_event(
            &mut transaction,
            &message_event,
            app.notification_service.clone(),
            app.integration_connection_service.clone(),
            app.third_party_item_service.clone(),
            app.slack_service.clone(),
        )
        .await
        .unwrap();

        transaction
            .commit()
            .await
            .context("Failed to commit transaction")
            .unwrap();

        slack_fetch_user_mock1.assert_hits(2);
        slack_fetch_user_mock2.assert_hits(2);
        slack_fetch_thread_mock_u01.assert_hits(1);
        slack_fetch_thread_mock_u02.assert_hits(1);
        slack_fetch_channel_mock.assert_hits(1);
        slack_fetch_team_mock.assert_hits(1);
        slack_list_users_in_usergroup_mock.assert_hits(1);

        let notifications_u02 = list_notifications(
            &client_u02,
            &app.api_address,
            vec![],
            false,
            None,
            None,
            false,
        )
        .await;
        assert_eq!(notifications_u02.len(), 1);
        assert_eq!(notifications_u02[0].status, NotificationStatus::Deleted);

        let notifications_u01 = list_notifications(
            &client_u01,
            &app.api_address,
            vec![],
            false,
            None,
            None,
            false,
        )
        .await;

        assert_eq!(notifications_u01.len(), 1);
        assert_eq!(
            notifications_u01[0].source_item.source_id,
            "1732535291.911209"
        );
        assert_eq!(notifications_u01[0].title, "Hello");
        assert_eq!(notifications_u01[0].status, NotificationStatus::Unread);
        assert_eq!(notifications_u01[0].kind, NotificationSourceKind::Slack);
        assert!(notifications_u01[0].last_read_at.is_none());
        assert!(notifications_u01[0].task_id.is_none());
        assert!(notifications_u01[0].snoozed_until.is_none());
        let ThirdPartyItemData::SlackThread(slack_thread) = &notifications_u01[0].source_item.data
        else {
            unreachable!("Unexpected item data");
        };
        assert_eq!(&slack_thread.channel.id.to_string(), "C05XXX");
        assert!(slack_thread.sender_profiles.contains_key("U01"));
        assert!(slack_thread.sender_profiles.contains_key("U02"));
        assert!(slack_thread.subscribed);
        assert_eq!(
            slack_thread
                .references
                .as_ref()
                .unwrap()
                .emojis
                .get(&SlackEmojiName("unknown1".to_string())),
            Some(&Some(SlackEmojiRef::Alias(SlackEmojiName(
                "wave".to_string()
            ))))
        );
    }

    #[rstest]
    #[case::user_ping_and_embedded_user_profiles(false, true)]
    #[case::usergroup_ping(true, false)]
    #[tokio::test]
    async fn test_handle_slack_message_in_channel_with_ping_to_known_user(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        mut nango_slack_connection: Box<NangoConnection>,
        mut message_event: Box<SlackPushEventCallback>,
        #[case] user_in_group: bool,
        #[case] embedded_user_profiles: bool,
    ) {
        let app = authenticated_app.await;
        let service = app.app.notification_service.read().await;
        let mut transaction = service.begin().await.unwrap();
        let slack_list_users_in_usergroup_mock = if user_in_group {
            add_usergroup_ref_in_message(&mut message_event, "G01");
            Some(mock_slack_list_users_in_usergroup(
                &app.app.slack_mock_server,
                "G01",
                "slack_list_users_in_usergroup_response.json",
            ))
        } else {
            add_user_ref_in_message(&mut message_event, "U01");
            None
        };
        nango_slack_connection.credentials.raw["authed_user"]["id"] = json!("U01");
        nango_slack_connection.credentials.raw["authed_user"]["access_token"] =
            json!("slack_test_user_access_token");
        nango_slack_connection.credentials.raw["team"]["id"] = json!("T01");
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            &settings,
            nango_slack_connection,
            None,
            Some(IntegrationConnectionContext::Slack(SlackContext {
                team_id: SlackTeamId("T01".to_string()),
            })),
        )
        .await;

        let slack_message_id = "1732535291.911209";
        mock_slack_get_chat_permalink(
            &app.app.slack_mock_server,
            "C05XXX",
            slack_message_id,
            "slack_get_chat_permalink_response.json",
        );
        let mut slack_fetch_user_mock1 = None;
        let mut slack_fetch_user_mock2 = None;
        if !embedded_user_profiles {
            // Fetch all users replying in the thread
            slack_fetch_user_mock1 = Some(mock_slack_fetch_user(
                &app.app.slack_mock_server,
                "U01",
                "slack_fetch_user_response.json",
            ));
            slack_fetch_user_mock2 = Some(mock_slack_fetch_user(
                &app.app.slack_mock_server,
                "U02",
                "slack_fetch_user_response.json",
            ));
        }
        let slack_fetch_thread_mock = mock_slack_fetch_thread(
            &app.app.slack_mock_server,
            "C05XXX",
            slack_message_id,
            slack_message_id,
            if embedded_user_profiles {
                "slack_embedded_user_profiles_fetch_thread_response.json"
            } else {
                "slack_fetch_thread_response.json"
            },
            true,
            None,
            "slack_test_user_access_token",
        );
        let slack_fetch_channel_mock = mock_slack_fetch_channel(
            &app.app.slack_mock_server,
            "C05XXX",
            "slack_fetch_channel_response.json",
        );
        let slack_fetch_team_mock = mock_slack_fetch_team(
            &app.app.slack_mock_server,
            "T01",
            "slack_fetch_team_response.json",
        );

        handle_slack_message_push_event(
            &mut transaction,
            &message_event,
            app.app.notification_service.clone(),
            app.app.integration_connection_service.clone(),
            app.app.third_party_item_service.clone(),
            app.app.slack_service.clone(),
        )
        .await
        .unwrap();

        transaction
            .commit()
            .await
            .context("Failed to commit transaction")
            .unwrap();

        if let Some(slack_fetch_user_mock1) = slack_fetch_user_mock1 {
            slack_fetch_user_mock1.assert();
        }
        if let Some(slack_fetch_user_mock2) = slack_fetch_user_mock2 {
            slack_fetch_user_mock2.assert();
        }
        slack_fetch_thread_mock.assert();
        slack_fetch_channel_mock.assert();
        slack_fetch_team_mock.assert();
        if let Some(slack_list_users_in_usergroup_mock) = slack_list_users_in_usergroup_mock {
            slack_list_users_in_usergroup_mock.assert();
        }

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

        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].source_item.source_id, "1732535291.911209");
        assert_eq!(notifications[0].title, "Hello");
        assert_eq!(notifications[0].status, NotificationStatus::Unread);
        assert_eq!(notifications[0].kind, NotificationSourceKind::Slack);
        assert!(notifications[0].last_read_at.is_none());
        assert!(notifications[0].task_id.is_none());
        assert!(notifications[0].snoozed_until.is_none());
        let ThirdPartyItemData::SlackThread(slack_thread) = &notifications[0].source_item.data
        else {
            unreachable!("Unexpected item data");
        };
        assert_eq!(&slack_thread.channel.id.to_string(), "C05XXX");
        assert!(slack_thread.sender_profiles.contains_key("U01"));
        assert!(slack_thread.sender_profiles.contains_key("U02"));
        assert!(slack_thread.subscribed);
    }

    #[rstest]
    #[tokio::test]
    async fn test_handle_slack_message_in_unknown_thread_without_ping(
        #[future] authenticated_app: AuthenticatedApp,
        message_in_thread_event: Box<SlackPushEventCallback>,
    ) {
        let app = authenticated_app.await;
        let service = app.app.notification_service.read().await;
        let mut transaction = service.begin().await.unwrap();

        handle_slack_message_push_event(
            &mut transaction,
            &message_in_thread_event,
            app.app.notification_service.clone(),
            app.app.integration_connection_service.clone(),
            app.app.third_party_item_service.clone(),
            app.app.slack_service.clone(),
        )
        .await
        .unwrap();

        transaction
            .commit()
            .await
            .context("Failed to commit transaction")
            .unwrap();

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
        assert!(notifications.is_empty());
    }

    #[rstest]
    #[case::is_2way_sync_was_subscribed_is_not_subscribed(true, true, false, false)]
    #[case::is_2way_sync_was_subscribed_is_subscribed(true, true, true, false)]
    #[case::is_2way_sync_was_not_subscribed_is_not_subscribed(true, false, false, false)]
    #[case::is_2way_sync_was_not_subscribed_is_subscribed(true, false, true, false)]
    #[case::is_1way_sync_was_subscribed_is_not_subscribed_pinged(false, true, false, true)]
    #[case::is_1way_sync_was_subscribed_is_subscribed_pinged(false, true, true, true)]
    #[case::is_1way_sync_was_not_subscribed_is_not_subscribed_pinged(false, false, false, true)]
    #[case::is_1way_sync_was_not_subscribed_is_subscribed_pinged(false, false, true, true)]
    #[case::is_1way_sync_was_subscribed_is_not_subscribed_not_pinged(false, true, false, false)]
    #[case::is_1way_sync_was_subscribed_is_subscribed_not_pinged(false, true, true, false)]
    #[case::is_1way_sync_was_not_subscribed_is_not_subscribed_not_pinged(
        false, false, false, false
    )]
    #[case::is_1way_sync_was_not_subscribed_is_subscribed_not_pinged(false, false, true, false)]
    #[tokio::test]
    async fn test_handle_slack_message_in_known_thread(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        mut nango_slack_connection: Box<NangoConnection>,
        mut message_in_thread_event: Box<SlackPushEventCallback>,
        mut slack_thread: Box<SlackThread>,
        #[case] is_2way_sync: bool,
        #[case] was_subscribed: bool,
        #[case] subscribed: bool,
        #[case] pinged: bool,
    ) {
        let app = authenticated_app.await;
        if pinged {
            add_user_ref_in_message(&mut message_in_thread_event, "U01");
            nango_slack_connection.credentials.raw = json!({
                "authed_user": { "id": "U01", "access_token": "slack_test_user_access_token" },
                "team": { "id": "T01" }
            });
        }
        let mut config = SlackConfig::enabled_as_notifications();
        config.message_config.is_2way_sync = is_2way_sync;
        let slack_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Slack(config),
            &settings,
            nango_slack_connection,
            None,
            Some(IntegrationConnectionContext::Slack(SlackContext {
                team_id: SlackTeamId("T01".to_string()),
            })),
        )
        .await;

        let service = app.app.notification_service.read().await;
        let mut transaction = service.begin().await.unwrap();

        let SlackPushEventCallback {
            event: SlackEventCallbackBody::Message(ref mut message),
            ..
        } = *message_in_thread_event
        else {
            unreachable!("Unexpected event type");
        };
        message.origin.thread_ts = Some(slack_thread.messages.first().origin.ts.clone());

        let slack_root_message_id = slack_thread.messages.first().origin.ts.to_string();
        let slack_message_id = message.origin.ts.to_string();
        // Fetch all users replying in the thread
        let slack_fetch_user_mock1 = mock_slack_fetch_user(
            &app.app.slack_mock_server,
            "U01",
            "slack_fetch_user_response.json",
        );
        let slack_fetch_user_mock2 = mock_slack_fetch_user(
            &app.app.slack_mock_server,
            "U02",
            "slack_fetch_user_response.json",
        );
        let slack_fetch_thread_mock = mock_slack_fetch_thread(
            &app.app.slack_mock_server,
            "C05XXX",
            &slack_root_message_id,
            &slack_message_id,
            "slack_fetch_thread_response.json",
            subscribed,
            Some(0),
            "slack_test_user_access_token",
        );
        let slack_fetch_channel_mock = mock_slack_fetch_channel(
            &app.app.slack_mock_server,
            "C05XXX",
            "slack_fetch_channel_response.json",
        );
        let slack_fetch_team_mock = mock_slack_fetch_team(
            &app.app.slack_mock_server,
            "T01",
            "slack_fetch_team_response.json",
        );

        let slack_first_unread_message_id = slack_thread.messages.last().origin.ts.clone();
        slack_thread.subscribed = was_subscribed;
        slack_thread.last_read = Some(slack_first_unread_message_id.clone());
        let _slack_get_chat_permalink_mock = mock_slack_get_chat_permalink(
            &app.app.slack_mock_server,
            "C05XXX",
            slack_first_unread_message_id.as_ref(),
            "slack_get_chat_permalink_response.json",
        );
        let existing_notification = create_notification_from_slack_thread(
            &app.app,
            &slack_thread,
            app.user.id,
            slack_integration_connection.id,
        )
        .await;
        assert_eq!(
            existing_notification.status,
            if was_subscribed {
                NotificationStatus::Deleted
            } else {
                NotificationStatus::Unsubscribed
            }
        );

        handle_slack_message_push_event(
            &mut transaction,
            &message_in_thread_event,
            app.app.notification_service.clone(),
            app.app.integration_connection_service.clone(),
            app.app.third_party_item_service.clone(),
            app.app.slack_service.clone(),
        )
        .await
        .unwrap();

        transaction
            .commit()
            .await
            .context("Failed to commit transaction")
            .unwrap();

        slack_fetch_user_mock1.assert();
        slack_fetch_user_mock2.assert();
        slack_fetch_thread_mock.assert();
        slack_fetch_channel_mock.assert();
        slack_fetch_team_mock.assert();

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

        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].source_item.source_id, "1732535291.911209");
        assert_eq!(notifications[0].title, "World");
        assert_eq!(
            notifications[0].status,
            if is_2way_sync {
                if subscribed {
                    NotificationStatus::Unread
                } else {
                    NotificationStatus::Unsubscribed
                }
            } else if (!was_subscribed && !pinged) || !subscribed {
                NotificationStatus::Unsubscribed
            } else {
                NotificationStatus::Unread
            }
        );
        assert_eq!(notifications[0].kind, NotificationSourceKind::Slack);
        let ThirdPartyItemData::SlackThread(updated_slack_thread) =
            &notifications[0].source_item.data
        else {
            unreachable!("Unexpected item data");
        };
        assert_eq!(
            updated_slack_thread.subscribed,
            if is_2way_sync {
                subscribed
            } else if !was_subscribed && !pinged {
                false
            } else {
                subscribed
            }
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_handle_slack_message_in_known_thread_from_known_user(
        settings: Settings,
        #[future] tested_app_with_local_auth: TestedApp,
        mut nango_slack_connection: Box<NangoConnection>,
        mut message_in_thread_event: Box<SlackPushEventCallback>,
        mut slack_thread: Box<SlackThread>,
    ) {
        let app = tested_app_with_local_auth.await;
        // `slack_thread` contains 2 messages from 2 different users:
        // - first (read) message from user U01
        // - second (unread) message from user U02
        //
        // U01 has already read the first message and when receiving the second message,
        // the notification will be marked as unread.
        // U02 has not read the first message and when receiving the second message,
        // as they are the sender, the notification will be marked as read.
        nango_slack_connection.credentials.raw = json !({
            "authed_user": { "id": "U02", "access_token": "slack_other_user_access_token" },
            "team": { "id": "T01" }
        });
        let (client_u02, user_u02) =
            create_user_and_login(&app, "john@doe.net".parse().unwrap(), "password").await;

        let slack_integration_connection_u02 = create_and_mock_integration_connection(
            &app,
            user_u02.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            &settings,
            nango_slack_connection.clone(),
            None,
            None,
        )
        .await;

        let service = app.notification_service.read().await;
        let mut transaction = service.begin().await.unwrap();

        let SlackPushEventCallback {
            event: SlackEventCallbackBody::Message(ref mut message),
            ..
        } = *message_in_thread_event
        else {
            unreachable!("Unexpected event type");
        };
        message.origin.thread_ts = Some(slack_thread.messages.first().origin.ts.clone());

        let slack_root_message_id = slack_thread.messages.first().origin.ts.to_string();
        let slack_message_id = message.origin.ts.to_string();
        // Fetch all users replying in the thread
        let slack_fetch_user_mock1 = mock_slack_fetch_user(
            &app.slack_mock_server,
            "U01",
            "slack_fetch_user_response.json",
        );
        let slack_fetch_user_mock2 = mock_slack_fetch_user(
            &app.slack_mock_server,
            "U02",
            "slack_fetch_user_response.json",
        );
        let slack_fetch_thread_mock_u02 = mock_slack_fetch_thread(
            &app.slack_mock_server,
            "C05XXX",
            &slack_root_message_id,
            &slack_message_id,
            "slack_fetch_thread_response.json",
            true,
            Some(1),
            "slack_other_user_access_token",
        );
        let slack_fetch_channel_mock = mock_slack_fetch_channel(
            &app.slack_mock_server,
            "C05XXX",
            "slack_fetch_channel_response.json",
        );
        let slack_fetch_team_mock = mock_slack_fetch_team(
            &app.slack_mock_server,
            "T01",
            "slack_fetch_team_response.json",
        );
        let slack_first_unread_message_id_u02 = slack_thread.messages.first().origin.ts.clone();
        mock_slack_get_chat_permalink(
            &app.slack_mock_server,
            "C05XXX",
            slack_first_unread_message_id_u02.as_ref(),
            "slack_get_chat_permalink_response.json",
        );

        // Creating user U01 and its Slack connection
        let (client_u01, user_u01) =
            create_user_and_login(&app, "jane@doe.net".parse().unwrap(), "password").await;
        nango_slack_connection.credentials.raw = json !({
            "authed_user": { "id": "U01", "access_token": "slack_test_user_access_token" },
            "team": { "id": "T01" }
        });
        let slack_integration_connection_u01 = create_and_mock_integration_connection(
            &app,
            user_u01.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            &settings,
            nango_slack_connection,
            None,
            None,
        )
        .await;
        let slack_first_unread_message_id_u01 = slack_thread.messages.last().origin.ts.clone();
        mock_slack_get_chat_permalink(
            &app.slack_mock_server,
            "C05XXX",
            slack_first_unread_message_id_u01.as_ref(),
            "slack_get_chat_permalink_response.json",
        );
        let slack_fetch_thread_mock_u01 = mock_slack_fetch_thread(
            &app.slack_mock_server,
            "C05XXX",
            &slack_root_message_id,
            &slack_message_id,
            "slack_fetch_thread_response.json",
            true,
            None,
            "slack_test_user_access_token",
        );

        slack_thread.subscribed = true;
        slack_thread.last_read = Some(slack_first_unread_message_id_u01.clone());
        let existing_notification_u01 = create_notification_from_slack_thread(
            &app,
            &slack_thread,
            user_u01.id,
            slack_integration_connection_u01.id,
        )
        .await;
        assert_eq!(
            existing_notification_u01.status,
            NotificationStatus::Deleted
        );

        slack_thread.subscribed = true;
        slack_thread.last_read = None;
        let existing_notification_u02 = create_notification_from_slack_thread(
            &app,
            &slack_thread,
            user_u02.id,
            slack_integration_connection_u02.id,
        )
        .await;
        assert_eq!(existing_notification_u02.status, NotificationStatus::Unread);

        handle_slack_message_push_event(
            &mut transaction,
            &message_in_thread_event,
            app.notification_service.clone(),
            app.integration_connection_service.clone(),
            app.third_party_item_service.clone(),
            app.slack_service.clone(),
        )
        .await
        .unwrap();

        transaction
            .commit()
            .await
            .context("Failed to commit transaction")
            .unwrap();

        slack_fetch_user_mock1.assert_hits(2);
        slack_fetch_user_mock2.assert_hits(2);
        slack_fetch_thread_mock_u01.assert_hits(1);
        slack_fetch_thread_mock_u02.assert_hits(1);
        slack_fetch_channel_mock.assert_hits(1);
        slack_fetch_team_mock.assert_hits(1);

        let notifications_u02 = list_notifications(
            &client_u02,
            &app.api_address,
            vec![],
            false,
            None,
            None,
            false,
        )
        .await;
        assert_eq!(notifications_u02.len(), 1);
        assert_eq!(notifications_u02[0].status, NotificationStatus::Deleted);

        let notifications_u01 = list_notifications(
            &client_u01,
            &app.api_address,
            vec![],
            false,
            None,
            None,
            false,
        )
        .await;
        assert_eq!(notifications_u01.len(), 1);
        assert_eq!(notifications_u01[0].status, NotificationStatus::Unread);
    }
}

fn add_user_ref_in_message(slack_push_message_event: &mut SlackPushEventCallback, user_id: &str) {
    let SlackPushEventCallback {
        event:
            SlackEventCallbackBody::Message(SlackMessageEvent {
                content: Some(ref mut content),
                ..
            }),
        ..
    } = *slack_push_message_event
    else {
        unreachable!("Unexpected event type");
    };
    content.blocks = Some(vec![SlackBlock::RichText(serde_json::json!({
    "block_id": "12345",
    "elements": [
        {
            "type": "rich_text_section",
            "elements": [
                {
                    "type": "user",
                    "user_id": user_id
                }
            ]
        },
    ]}))]);
}

fn add_usergroup_ref_in_message(
    slack_push_message_event: &mut SlackPushEventCallback,
    usergroup_id: &str,
) {
    let SlackPushEventCallback {
        event:
            SlackEventCallbackBody::Message(SlackMessageEvent {
                content: Some(ref mut content),
                ..
            }),
        ..
    } = *slack_push_message_event
    else {
        unreachable!("Unexpected event type");
    };
    content.blocks = Some(vec![SlackBlock::RichText(serde_json::json!({
    "block_id": "12345",
    "elements": [
        {
            "type": "rich_text_section",
            "elements": [
                {
                    "type": "usergroup",
                    "usergroup_id": usergroup_id
                }
            ]
        },
    ]}))]);
}
