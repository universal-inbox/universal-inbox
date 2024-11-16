use chrono::{TimeZone, Utc};
use httpmock::Method::POST;
use rstest::*;
use serde_json::json;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::google_mail::GoogleMailConfig,
    },
    notification::{service::NotificationPatch, Notification, NotificationStatus},
    third_party::integrations::google_mail::{GoogleMailThread, GOOGLE_MAIL_INBOX_LABEL},
};

use universal_inbox_api::{configuration::Settings, integrations::oauth2::NangoConnection};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_and_mock_integration_connection, nango_google_mail_connection,
    },
    notification::google_mail::{
        create_notification_from_google_mail_thread, google_mail_thread_get_123,
        mock_google_mail_thread_modify_service,
    },
    rest::{patch_resource, patch_resource_response},
    settings,
};

mod patch_resource {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_google_mail_notification_status_as_deleted(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        google_mail_thread_get_123: GoogleMailThread,
        nango_google_mail_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let google_mail_config = GoogleMailConfig::enabled();
        let synced_label_id = google_mail_config.synced_label.id.clone();
        let google_mail_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
            &settings,
            nango_google_mail_connection,
            None,
            None,
        )
        .await;

        let expected_notification = create_notification_from_google_mail_thread(
            &app.app,
            &google_mail_thread_get_123,
            app.user.id,
            google_mail_integration_connection.id,
        )
        .await;

        let google_mail_thread_modify_mock = mock_google_mail_thread_modify_service(
            &app.app.google_mail_mock_server,
            &google_mail_thread_get_123.id,
            vec![],
            vec![GOOGLE_MAIL_INBOX_LABEL, &synced_label_id],
        );

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
        google_mail_thread_modify_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_google_mail_notification_status_as_deleted_with_google_mail_error_response(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        google_mail_thread_get_123: GoogleMailThread,
        nango_google_mail_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let google_mail_config = GoogleMailConfig::enabled();
        let synced_label_id = google_mail_config.synced_label.id.clone();
        let google_mail_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
            &settings,
            nango_google_mail_connection,
            None,
            None,
        )
        .await;

        let expected_notification = create_notification_from_google_mail_thread(
            &app.app,
            &google_mail_thread_get_123,
            app.user.id,
            google_mail_integration_connection.id,
        )
        .await;

        let google_mail_thread_modify_mock = app.app.google_mail_mock_server.mock(|when, then| {
            when.method(POST)
                .path(format!(
                    "/users/me/threads/{}/modify",
                    &google_mail_thread_get_123.id
                ))
                .body(
                    json!({
                        "addLabelIds": Vec::<String>::new(),
                        "removeLabelIds": vec![GOOGLE_MAIL_INBOX_LABEL, &synced_label_id]
                    })
                    .to_string(),
                )
                .header("authorization", "Bearer google_mail_test_access_token");
            then.status(403).header("content-type", "application/json");
        });

        let patch_response = patch_resource_response(
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

        assert_eq!(patch_response.status(), 500);
        let body = patch_response.text().await.unwrap();
        assert_eq!(
            body,
            r#"{"message":"Failed to modify Google Mail thread `123` labels"}"#
        );
        google_mail_thread_modify_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_google_mail_notification_status_as_unsubscribed(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        google_mail_thread_get_123: GoogleMailThread,
        nango_google_mail_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let google_mail_config = GoogleMailConfig::enabled();
        let synced_label_id = google_mail_config.synced_label.id.clone();
        let google_mail_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
            &settings,
            nango_google_mail_connection,
            None,
            None,
        )
        .await;

        let expected_notification = create_notification_from_google_mail_thread(
            &app.app,
            &google_mail_thread_get_123,
            app.user.id,
            google_mail_integration_connection.id,
        )
        .await;

        // Unsubscribed notifications are only archived on Google Mail.
        // Universal Inbox will ignore new messages and archive them during the next sync
        let google_mail_thread_modify_mock = mock_google_mail_thread_modify_service(
            &app.app.google_mail_mock_server,
            &google_mail_thread_get_123.id,
            vec![],
            vec![GOOGLE_MAIL_INBOX_LABEL, &synced_label_id],
        );

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
        google_mail_thread_modify_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_google_mail_notification_snoozed_until(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        google_mail_thread_get_123: GoogleMailThread,
        nango_google_mail_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let google_mail_config = GoogleMailConfig::enabled();

        let google_mail_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
            &settings,
            nango_google_mail_connection,
            None,
            None,
        )
        .await;

        let expected_notification = create_notification_from_google_mail_thread(
            &app.app,
            &google_mail_thread_get_123,
            app.user.id,
            google_mail_integration_connection.id,
        )
        .await;
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();

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
