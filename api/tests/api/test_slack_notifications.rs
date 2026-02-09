#![allow(clippy::too_many_arguments)]
use chrono::{TimeZone, Utc};
use rstest::*;
use serde_json::json;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::slack::SlackConfig,
    },
    notification::{Notification, NotificationStatus, service::NotificationPatch},
    third_party::integrations::slack::SlackStar,
};

use universal_inbox_api::{configuration::Settings, integrations::oauth2::NangoConnection};
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{create_and_mock_integration_connection, nango_slack_connection},
    notification::slack::{
        create_notification_from_slack_star, mock_slack_stars_remove, slack_star_added,
    },
    rest::{get_resource, patch_resource, patch_resource_response},
    settings,
};

mod patch_resource_slack_star {
    use super::*;
    use pretty_assertions::assert_eq;

    #[rstest]
    #[tokio::test]
    async fn test_patch_slack_notification_status_as_deleted(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_slack_connection: Box<NangoConnection>,
        slack_star_added: Box<SlackStar>,
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

        let _slack_stars_remove_mock =
            mock_slack_stars_remove(&app.app.slack_mock_server, "C05XXX", "1707686216.825719")
                .await;

        let expected_notification = create_notification_from_slack_star(
            &app.app,
            &slack_star_added,
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
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_slack_notification_status_as_unsubscribed(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_slack_connection: Box<NangoConnection>,
        slack_star_added: Box<SlackStar>,
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

        let _slack_stars_remove_mock =
            mock_slack_stars_remove(&app.app.slack_mock_server, "C05XXX", "1707686216.825719")
                .await;
        let expected_notification = create_notification_from_slack_star(
            &app.app,
            &slack_star_added,
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
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_slack_notification_status_with_slack_error(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_slack_connection: Box<NangoConnection>,
        slack_star_added: Box<SlackStar>,
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

        Mock::given(method("POST"))
            .and(path("/stars.remove"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&app.app.slack_mock_server)
            .await;

        let expected_notification = create_notification_from_slack_star(
            &app.app,
            &slack_star_added,
            app.user.id,
            slack_integration_connection.id,
        )
        .await;

        let patch_response = patch_resource_response(
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

        assert_eq!(patch_response.status(), 500);

        let body = patch_response
            .text()
            .await
            .expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": "Failed to remove Slack star" }).to_string()
        );

        let notification: Box<Notification> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.id.into(),
        )
        .await;
        assert_eq!(notification.status, NotificationStatus::Unread);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_slack_notification_snoozed_until(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        nango_slack_connection: Box<NangoConnection>,
        slack_star_added: Box<SlackStar>,
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
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();

        let expected_notification = create_notification_from_slack_star(
            &app.app,
            &slack_star_added,
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
