use chrono::{TimeZone, Utc};
use rstest::*;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::{
    notification::{Notification, NotificationMetadata, NotificationPatch, NotificationStatus},
    task::{
        integrations::todoist::{get_task_html_url, TodoistItem},
        Task, TaskStatus,
    },
};

use crate::helpers::{
    rest::{create_resource, get_resource, patch_resource, patch_resource_response},
    task::todoist::{
        create_task_from_todoist_item, mock_todoist_delete_item_service, todoist_item,
    },
    tested_app, TestedApp,
};

mod patch_notification {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_notification_status_as_deleted(
        #[future] tested_app: TestedApp,
        todoist_item: Box<TodoistItem>,
    ) {
        let app = tested_app.await;
        let existing_todoist_task_creation =
            create_task_from_todoist_item(&app.app_address, &todoist_item).await;
        let existing_todoist_task = existing_todoist_task_creation.task;
        assert_eq!(existing_todoist_task.status, TaskStatus::Active);
        let existing_todoist_notification = existing_todoist_task_creation.notification.unwrap();
        let todoist_mock = mock_todoist_delete_item_service(
            &app.todoist_mock_server,
            &existing_todoist_task.source_id,
        );

        let patched_notification = patch_resource(
            &app.app_address,
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
                ..existing_todoist_notification
            })
        );
        todoist_mock.assert();

        let deleted_task: Box<Task> =
            get_resource(&app.app_address, "tasks", existing_todoist_task.id.into()).await;
        assert_eq!(deleted_task.status, TaskStatus::Deleted);
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_todoist_notification_status(#[future] tested_app: TestedApp) {
        let app = tested_app.await;
        let expected_notification = Box::new(Notification {
            id: Uuid::new_v4().into(),
            title: "task 1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: get_task_html_url("1234"),
            metadata: NotificationMetadata::Todoist,
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
            task_id: None,
            task_source_id: None,
        });
        let created_notification: Box<Notification> = create_resource(
            &app.app_address,
            "notifications",
            expected_notification.clone(),
        )
        .await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_resource_response(
            &app.app_address,
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