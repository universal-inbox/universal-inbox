use chrono::{TimeZone, Utc};
use rstest::*;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::google_mail::GoogleMailConfig,
    },
    notification::{Notification, NotificationStatus, service::NotificationPatch},
    third_party::integrations::google_mail::{GOOGLE_MAIL_INBOX_LABEL, GoogleMailThread},
};

use universal_inbox_api::{configuration::Settings, integrations::oauth2::NangoConnection};
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
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

        let _google_mail_thread_modify_mock = mock_google_mail_thread_modify_service(
            &app.app.google_mail_mock_server,
            &google_mail_thread_get_123.id,
            vec![],
            vec![GOOGLE_MAIL_INBOX_LABEL, &synced_label_id],
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
    async fn test_patch_google_mail_notification_status_as_deleted_with_google_mail_error_response(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        google_mail_thread_get_123: GoogleMailThread,
        nango_google_mail_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let google_mail_config = GoogleMailConfig::enabled();
        let _synced_label_id = google_mail_config.synced_label.id.clone();
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

        Mock::given(method("POST"))
            .and(path(format!(
                "/users/me/threads/{}/modify",
                &google_mail_thread_get_123.id
            )))
            .respond_with(ResponseTemplate::new(403))
            .mount(&app.app.google_mail_mock_server)
            .await;

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
        assert_eq!(body, r#"{"message":"Failed to modify Google Mail thread"}"#);
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
        let _google_mail_thread_modify_mock = mock_google_mail_thread_modify_service(
            &app.app.google_mail_mock_server,
            &google_mail_thread_get_123.id,
            vec![],
            vec![GOOGLE_MAIL_INBOX_LABEL, &synced_label_id],
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
