use chrono::{TimeZone, Timelike, Utc};
use rstest::*;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::todoist::TodoistConfig,
    },
    notification::{
        service::NotificationPatch, Notification, NotificationMetadata, NotificationStatus,
    },
    task::{Task, TaskStatus},
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
    integration_connection::{create_and_mock_integration_connection, nango_todoist_connection},
    rest::{create_resource, get_resource, patch_resource, patch_resource_response},
    settings,
    task::todoist::{
        mock_todoist_delete_item_service, mock_todoist_sync_resources_service,
        sync_todoist_projects_response, todoist_item,
    },
};

mod patch_notification {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_notification_status_as_deleted(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
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
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TodoistItem(TodoistItem {
                    project_id: "1111".to_string(), // ie. "Inbox"
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..*todoist_item.clone()
                }),
                integration_connection_id: integration_connection.id,
            }),
        )
        .await;
        let existing_todoist_task = creation.task.as_ref().unwrap().clone();

        assert_eq!(existing_todoist_task.status, TaskStatus::Active);
        let existing_todoist_notification = creation.notification.as_ref().unwrap().clone();
        let todoist_mock = mock_todoist_delete_item_service(
            &app.app.todoist_mock_server,
            &existing_todoist_task.source_item.source_id,
        );

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            existing_todoist_notification.id.into(),
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
                ..existing_todoist_notification.clone()
            })
        );
        todoist_mock.assert();

        let deleted_task: Box<Task> = get_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_todoist_task.id.into(),
        )
        .await;
        assert_eq!(deleted_task.status, TaskStatus::Deleted);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_notification_status_as_snoozed(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_item: Box<TodoistItem>,
        sync_todoist_projects_response: TodoistSyncResponse,
        nango_todoist_connection: Box<NangoConnection>,
    ) {
        let app = authenticated_app.await;
        let integration_connection = create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            &settings.oauth2.nango_secret_key,
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
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                updated_at: Utc::now().with_nanosecond(0).unwrap(),
                user_id: app.user.id,
                data: ThirdPartyItemData::TodoistItem(TodoistItem {
                    project_id: "1111".to_string(), // ie. "Inbox"
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..*todoist_item.clone()
                }),
                integration_connection_id: integration_connection.id,
            }),
        )
        .await;
        let existing_todoist_task = creation.task.as_ref().unwrap().clone();
        assert_eq!(existing_todoist_task.status, TaskStatus::Active);

        let existing_todoist_notification = creation.notification.as_ref().unwrap().clone();
        let snoozed_time = Utc.with_ymd_and_hms(2022, 1, 1, 1, 2, 3).unwrap();

        let patched_notification = patch_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            existing_todoist_notification.id.into(),
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
                ..existing_todoist_notification.clone()
            })
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_notification_status(#[future] authenticated_app: AuthenticatedApp) {
        let app = authenticated_app.await;
        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            user_id: app.user.id,
            title: "task 1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            metadata: NotificationMetadata::Todoist,
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            details: None,
            task_id: None,
        });
        let created_notification: Box<Notification> = create_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_resource_response(
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

        assert_eq!(response.status(), 400);

        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({
                "message": format!(
                    "Unsupported action: Cannot update the status of Todoist notification {}, update task's project",
                    expected_notification.id
                )
            })
            .to_string()
        );
    }
}
