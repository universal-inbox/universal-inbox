use chrono::{TimeZone, Utc};
use httpmock::Method::POST;
use rstest::*;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{google_mail::GoogleMailConfig, todoist::TodoistConfig},
    },
    notification::{
        integrations::google_mail::{GoogleMailThread, GOOGLE_MAIL_INBOX_LABEL},
        service::NotificationPatch,
        Notification, NotificationStatus,
    },
    task::Task,
    third_party::{
        integrations::todoist::TodoistItem,
        item::{ThirdPartyItem, ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{oauth2::NangoConnection, todoist::TodoistSyncResponse},
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_and_mock_integration_connection, nango_google_mail_connection,
        nango_todoist_connection,
    },
    notification::google_mail::{
        google_mail_thread_get_123, mock_google_mail_thread_modify_service,
    },
    rest::{create_resource, get_resource, patch_resource, patch_resource_response},
    settings,
    task::todoist::{
        mock_todoist_sync_resources_service, sync_todoist_projects_response, todoist_item,
    },
};

mod patch_resource {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_google_mail_notification_status_as_deleted(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        google_mail_thread_get_123: GoogleMailThread,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        nango_todoist_connection: Box<NangoConnection>,
        nango_google_mail_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let google_mail_config = GoogleMailConfig::enabled();
        let synced_label_id = google_mail_config.synced_label.id.clone();
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
            &settings,
            nango_google_mail_connection,
            None,
        )
        .await;

        let expected_notification = Box::new(google_mail_thread_get_123.into_notification(
            app.user.id,
            None,
            &synced_label_id,
        ));
        let google_mail_thread_modify_mock = mock_google_mail_thread_modify_service(
            &app.app.google_mail_mock_server,
            &expected_notification.source_id,
            vec![],
            vec![GOOGLE_MAIL_INBOX_LABEL, &synced_label_id],
        );

        let integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            &settings,
            nango_todoist_connection,
            None,
        )
        .await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        );

        let creation: Box<ThirdPartyItemCreationResult> = create_resource(
            &app.client,
            &app.app.api_address,
            "third_party/items",
            Box::new(ThirdPartyItem {
                id: Uuid::new_v4().into(),
                source_id: todoist_item.id.clone(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TodoistItem(TodoistItem {
                    project_id: "2222".to_string(), // ie. "Project2"
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..*todoist_item.clone()
                }),
                integration_connection_id: integration_connection.id,
            }),
        )
        .await;
        let existing_todoist_task = creation.task.as_ref().unwrap();

        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
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
                ..*created_notification
            })
        );
        google_mail_thread_modify_mock.assert();

        let task: Box<Task> = get_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_todoist_task.id.into(),
        )
        .await;
        assert_eq!(task.status, existing_todoist_task.status);
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
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
            &settings,
            nango_google_mail_connection,
            None,
        )
        .await;

        let expected_notification = Box::new(google_mail_thread_get_123.into_notification(
            app.user.id,
            None,
            &synced_label_id,
        ));
        let google_mail_thread_modify_mock = app.app.google_mail_mock_server.mock(|when, then| {
            when.method(POST)
                .path(format!(
                    "/users/me/threads/{}/modify",
                    &expected_notification.source_id
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

        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let patch_response = patch_resource_response(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
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
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.integrations.oauth2.nango_secret_key,
            IntegrationConnectionConfig::GoogleMail(google_mail_config.clone()),
            &settings,
            nango_google_mail_connection,
            None,
        )
        .await;

        let expected_notification = Box::new(google_mail_thread_get_123.into_notification(
            app.user.id,
            None,
            &synced_label_id,
        ));
        // Unsubscribed notifications are only archived on Google Mail.
        // Universal Inbox will ignore new messages and archive them during the next sync
        let google_mail_thread_modify_mock = mock_google_mail_thread_modify_service(
            &app.app.google_mail_mock_server,
            &expected_notification.source_id,
            vec![],
            vec![GOOGLE_MAIL_INBOX_LABEL, &synced_label_id],
        );

        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
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
                ..*created_notification
            })
        );
        google_mail_thread_modify_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_google_mail_notification_snoozed_until(
        #[future] authenticated_app: AuthenticatedApp,
        google_mail_thread_get_123: GoogleMailThread,
    ) {
        let app = authenticated_app.await;
        let google_mail_config = GoogleMailConfig::enabled();
        let synced_label_id = google_mail_config.synced_label.id.clone();

        let expected_notification = Box::new(google_mail_thread_get_123.into_notification(
            app.user.id,
            None,
            &synced_label_id,
        ));
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();
        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            created_notification.id.into(),
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
                ..*created_notification
            })
        );
    }
}
