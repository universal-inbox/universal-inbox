#![allow(clippy::too_many_arguments)]
use rstest::*;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::slack::SlackConfig,
    },
    notification::{NotificationSourceKind, NotificationStatus},
    third_party::integrations::slack::SlackThread,
};

use universal_inbox_api::{configuration::Settings, integrations::oauth2::NangoConnection};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{create_and_mock_integration_connection, nango_slack_connection},
    notification::{
        slack::{
            create_notification_from_slack_thread, mock_slack_fetch_channel, mock_slack_fetch_team,
            mock_slack_fetch_thread, mock_slack_fetch_user, mock_slack_get_chat_permalink,
            slack_thread,
        },
        sync_notifications,
    },
    settings,
};

mod sync_slack_thread_notifications {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_sync_slack_thread_marked_as_read(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_slack_connection: Box<NangoConnection>,
        slack_thread: Box<SlackThread>,
    ) {
        let app = authenticated_app.await;
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

        // Create a notification from a slack thread (will be Unread because
        // last_read is set to the first message, not the last)
        let mut unread_slack_thread = *slack_thread.clone();
        unread_slack_thread.last_read = None;
        let existing_notification = create_notification_from_slack_thread(
            &app.app,
            &unread_slack_thread,
            app.user.id,
            slack_integration_connection.id,
        )
        .await;
        assert_eq!(existing_notification.status, NotificationStatus::Unread);

        let first_message_id = slack_thread.messages.first().origin.ts.to_string();
        let last_message_id = slack_thread.messages.last().origin.ts.to_string();

        // Mock Slack API to return thread with last_read matching the last message
        // (i.e., user read all messages in Slack)
        mock_slack_fetch_thread(
            &app.app.slack_mock_server,
            "C05XXX",
            &first_message_id,
            &last_message_id,
            "slack_fetch_thread_response.json",
            true,
            Some(1), // last_read = last message index (message at index 1)
            "slack_test_user_access_token",
        )
        .await;
        mock_slack_fetch_channel(
            &app.app.slack_mock_server,
            "C05XXX",
            "slack_fetch_channel_response.json",
        )
        .await;
        mock_slack_fetch_team(
            &app.app.slack_mock_server,
            "T05XXX",
            "slack_fetch_team_response.json",
        )
        .await;
        mock_slack_fetch_user(
            &app.app.slack_mock_server,
            "U01",
            "slack_fetch_user_response.json",
        )
        .await;
        mock_slack_fetch_user(
            &app.app.slack_mock_server,
            "U02",
            "slack_fetch_user_response.json",
        )
        .await;
        mock_slack_get_chat_permalink(
            &app.app.slack_mock_server,
            "C05XXX",
            &last_message_id,
            "slack_get_chat_permalink_response.json",
        )
        .await;

        let synced_notifications = sync_notifications(
            &app.client,
            &app.app.api_address,
            Some(NotificationSourceKind::Slack),
            false,
        )
        .await;

        assert_eq!(synced_notifications.len(), 1);
        assert_eq!(synced_notifications[0].status, NotificationStatus::Deleted);
    }

    #[rstest]
    #[tokio::test]
    async fn test_sync_slack_thread_marked_as_unsubscribed(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_slack_connection: Box<NangoConnection>,
        slack_thread: Box<SlackThread>,
    ) {
        let app = authenticated_app.await;
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

        let mut unread_slack_thread = *slack_thread.clone();
        unread_slack_thread.last_read = None;
        let existing_notification = create_notification_from_slack_thread(
            &app.app,
            &unread_slack_thread,
            app.user.id,
            slack_integration_connection.id,
        )
        .await;
        assert_eq!(existing_notification.status, NotificationStatus::Unread);

        let first_message_id = slack_thread.messages.first().origin.ts.to_string();
        let last_message_id = slack_thread.messages.last().origin.ts.to_string();

        // Mock Slack API to return thread with subscribed: false
        mock_slack_fetch_thread(
            &app.app.slack_mock_server,
            "C05XXX",
            &first_message_id,
            &last_message_id,
            "slack_fetch_thread_response.json",
            false, // subscribed = false (user unsubscribed in Slack)
            None,
            "slack_test_user_access_token",
        )
        .await;
        mock_slack_fetch_channel(
            &app.app.slack_mock_server,
            "C05XXX",
            "slack_fetch_channel_response.json",
        )
        .await;
        mock_slack_fetch_team(
            &app.app.slack_mock_server,
            "T05XXX",
            "slack_fetch_team_response.json",
        )
        .await;
        mock_slack_fetch_user(
            &app.app.slack_mock_server,
            "U01",
            "slack_fetch_user_response.json",
        )
        .await;
        mock_slack_fetch_user(
            &app.app.slack_mock_server,
            "U02",
            "slack_fetch_user_response.json",
        )
        .await;
        mock_slack_get_chat_permalink(
            &app.app.slack_mock_server,
            "C05XXX",
            &first_message_id,
            "slack_get_chat_permalink_response.json",
        )
        .await;

        let synced_notifications = sync_notifications(
            &app.client,
            &app.app.api_address,
            Some(NotificationSourceKind::Slack),
            false,
        )
        .await;

        assert_eq!(synced_notifications.len(), 1);
        assert_eq!(
            synced_notifications[0].status,
            NotificationStatus::Unsubscribed
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_sync_slack_thread_no_change(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_slack_connection: Box<NangoConnection>,
        slack_thread: Box<SlackThread>,
    ) {
        let app = authenticated_app.await;
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

        let mut unread_slack_thread = *slack_thread.clone();
        unread_slack_thread.last_read = None;
        let existing_notification = create_notification_from_slack_thread(
            &app.app,
            &unread_slack_thread,
            app.user.id,
            slack_integration_connection.id,
        )
        .await;
        assert_eq!(existing_notification.status, NotificationStatus::Unread);

        let first_message_id = slack_thread.messages.first().origin.ts.to_string();
        let last_message_id = slack_thread.messages.last().origin.ts.to_string();

        // Mock Slack API to return thread with same status (still unread)
        mock_slack_fetch_thread(
            &app.app.slack_mock_server,
            "C05XXX",
            &first_message_id,
            &last_message_id,
            "slack_fetch_thread_response.json",
            true,
            None, // No last_read => still unread
            "slack_test_user_access_token",
        )
        .await;
        mock_slack_fetch_channel(
            &app.app.slack_mock_server,
            "C05XXX",
            "slack_fetch_channel_response.json",
        )
        .await;
        mock_slack_fetch_team(
            &app.app.slack_mock_server,
            "T05XXX",
            "slack_fetch_team_response.json",
        )
        .await;
        mock_slack_fetch_user(
            &app.app.slack_mock_server,
            "U01",
            "slack_fetch_user_response.json",
        )
        .await;
        mock_slack_fetch_user(
            &app.app.slack_mock_server,
            "U02",
            "slack_fetch_user_response.json",
        )
        .await;
        mock_slack_get_chat_permalink(
            &app.app.slack_mock_server,
            "C05XXX",
            &first_message_id,
            "slack_get_chat_permalink_response.json",
        )
        .await;

        let synced_notifications = sync_notifications(
            &app.client,
            &app.app.api_address,
            Some(NotificationSourceKind::Slack),
            false,
        )
        .await;

        assert_eq!(synced_notifications.len(), 1);
        assert_eq!(synced_notifications[0].status, NotificationStatus::Unread);
    }
}
