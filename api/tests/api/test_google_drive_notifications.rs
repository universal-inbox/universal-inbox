use rstest::*;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::google_drive::GoogleDriveConfig,
    },
    notification::{Notification, NotificationStatus, service::NotificationPatch},
    third_party::integrations::google_drive::GoogleDriveComment,
};

use universal_inbox_api::{configuration::Settings, integrations::oauth2::NangoConnection};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, nango_google_drive_connection,
    },
    notification::google_drive::{
        create_notification_from_google_drive_comment, google_drive_comment_123,
    },
    rest::patch_resource,
    settings,
};

mod patch_resource {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_google_drive_notification_status_as_deleted(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        google_drive_comment_123: GoogleDriveComment,
        nango_google_drive_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let google_drive_config = GoogleDriveConfig::enabled();
        let google_drive_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleDrive(google_drive_config.clone()),
            &settings,
            nango_google_drive_connection,
            None,
            None,
        )
        .await;

        let google_drive_notification = create_notification_from_google_drive_comment(
            &app.app,
            &google_drive_comment_123,
            app.user.id,
            google_drive_integration_connection.id,
        )
        .await;

        let patched_notification: Box<Notification> = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            google_drive_notification.id.into(),
            &NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(patched_notification.status, NotificationStatus::Deleted);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_google_drive_notification_status_as_unsubscribed(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        google_drive_comment_123: GoogleDriveComment,
        nango_google_drive_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let google_drive_config = GoogleDriveConfig::enabled();
        let google_drive_integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleDrive(google_drive_config.clone()),
            &settings,
            nango_google_drive_connection,
            None,
            None,
        )
        .await;

        let google_drive_notification = create_notification_from_google_drive_comment(
            &app.app,
            &google_drive_comment_123,
            app.user.id,
            google_drive_integration_connection.id,
        )
        .await;

        let patched_notification: Box<Notification> = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            google_drive_notification.id.into(),
            &NotificationPatch {
                status: Some(NotificationStatus::Unsubscribed),
                ..Default::default()
            },
        )
        .await;

        assert_eq!(
            patched_notification.status,
            NotificationStatus::Unsubscribed
        );
    }
}
