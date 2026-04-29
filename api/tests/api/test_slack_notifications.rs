#![allow(clippy::too_many_arguments)]
use chrono::{TimeZone, Utc};
use rstest::*;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::slack::SlackConfig,
    },
    notification::{Notification, NotificationStatus, service::NotificationPatch},
};

use universal_inbox_api::configuration::Settings;

use crate::helpers::integration_connection::OAuthCredentialFixture;
use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{create_and_mock_integration_connection, slack_oauth_credential},
    rest::patch_resource,
    settings,
};

mod patch_resource_slack_thread {
    use super::*;
    use pretty_assertions::assert_eq;

    use universal_inbox::third_party::{
        integrations::slack::SlackThread, item::ThirdPartyItemData,
    };

    use crate::helpers::notification::slack::{
        create_notification_from_slack_thread, slack_thread,
    };

    #[rstest]
    #[tokio::test]
    async fn test_patch_slack_notification_status_as_deleted(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        slack_oauth_credential: OAuthCredentialFixture,
        slack_thread: Box<SlackThread>,
    ) {
        let app = authenticated_app.await;
        let slack_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            &settings,
            slack_oauth_credential,
            None,
            None,
        )
        .await;

        let expected_notification = create_notification_from_slack_thread(
            &app.app,
            &slack_thread,
            app.user.id,
            slack_integration_connection.id,
        )
        .await;

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.id.into(),
            &NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                status: NotificationStatus::Deleted,
                ..*expected_notification
            })
        );
        let ThirdPartyItemData::SlackThread(slack_thread) = patched_notification.source_item.data
        else {
            unreachable!(
                "Expected SlackThread data, got {:?}",
                patched_notification.source_item.data
            );
        };
        assert_eq!(
            slack_thread.last_read,
            Some(slack_thread.messages.last().origin.ts.clone())
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_slack_notification_status_as_unsubscribed(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        slack_oauth_credential: OAuthCredentialFixture,
        slack_thread: Box<SlackThread>,
    ) {
        let app = authenticated_app.await;
        let slack_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            &settings,
            slack_oauth_credential,
            None,
            None,
        )
        .await;

        let expected_notification = create_notification_from_slack_thread(
            &app.app,
            &slack_thread,
            app.user.id,
            slack_integration_connection.id,
        )
        .await;

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.id.into(),
            &NotificationPatch {
                status: Some(NotificationStatus::Unsubscribed),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                status: NotificationStatus::Unsubscribed,
                ..*expected_notification
            })
        );
        let ThirdPartyItemData::SlackThread(slack_thread) = patched_notification.source_item.data
        else {
            unreachable!(
                "Expected SlackThread data, got {:?}",
                patched_notification.source_item.data
            );
        };
        assert!(!slack_thread.subscribed);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_slack_notification_snoozed_until(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        slack_oauth_credential: OAuthCredentialFixture,
        slack_thread: Box<SlackThread>,
    ) {
        let app = authenticated_app.await;
        let slack_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
            &settings,
            slack_oauth_credential,
            None,
            None,
        )
        .await;
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();

        let expected_notification = create_notification_from_slack_thread(
            &app.app,
            &slack_thread,
            app.user.id,
            slack_integration_connection.id,
        )
        .await;

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.id.into(),
            &NotificationPatch {
                snoozed_until: Some(snoozed_time),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_notification,
            Box::new(Notification {
                snoozed_until: Some(snoozed_time),
                ..*expected_notification
            })
        );
    }
}
