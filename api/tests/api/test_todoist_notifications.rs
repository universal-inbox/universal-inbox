use chrono::{TimeZone, Utc};
use httpmock::Method::DELETE;
use rstest::*;
use serde_json::json;

use universal_inbox::{
    integrations::todoist::TodoistTask, Notification, NotificationMetadata, NotificationPatch,
    NotificationStatus,
};

use crate::helpers::{
    create_notification, get_notification, patch_notification, patch_notification_response,
    tested_app, todoist::todoist_task, TestedApp,
};

mod patch_notification {
    use super::*;

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_status_as_deleted(
        #[future] tested_app: TestedApp,
        todoist_task: Box<TodoistTask>,
        #[values(205, 304, 404)] todoist_status_code: u16,
    ) {
        let app = tested_app.await;
        let todoist_mark_thread_as_read_mock = app.todoist_mock_server.mock(|when, then| {
            when.method(DELETE)
                .path("/tasks/1234")
                .header("authorization", "Bearer todoist_test_token");
            then.status(todoist_status_code);
        });
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "task 1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: Some(todoist_task.url.clone()),
            metadata: NotificationMetadata::Todoist(*todoist_task),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
        });
        let created_notification =
            create_notification(&app.app_address, expected_notification.clone()).await;

        assert_eq!(created_notification, expected_notification);

        let patched_notification = patch_notification(
            &app.app_address,
            created_notification.id,
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
        todoist_mark_thread_as_read_mock.assert();
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_status_as_unsubscribed(
        #[future] tested_app: TestedApp,
        todoist_task: Box<TodoistTask>,
    ) {
        let app = tested_app.await;
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "task 1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: Some(todoist_task.url.clone()),
            metadata: NotificationMetadata::Todoist(*todoist_task),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
        });
        let created_notification =
            create_notification(&app.app_address, expected_notification.clone()).await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_notification_response(
            &app.app_address,
            created_notification.id,
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
                "message":
                    format!("Unsupported action: Cannot unsubscribe from Todoist task `1234`")
            })
            .to_string()
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_patch_notification_status_as_deleted_with_todoist_api_error(
        #[future] tested_app: TestedApp,
        todoist_task: Box<TodoistTask>,
    ) {
        let app = tested_app.await;
        let todoist_mark_thread_as_read_mock = app.todoist_mock_server.mock(|when, then| {
            when.method(DELETE)
                .path("/tasks/1234")
                .header("authorization", "Bearer todoist_test_token");
            then.status(403);
        });
        let expected_notification = Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "task 1".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1234".to_string(),
            source_html_url: Some(todoist_task.url.clone()),
            metadata: NotificationMetadata::Todoist(*todoist_task),
            updated_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
        });
        let created_notification =
            create_notification(&app.app_address, expected_notification.clone()).await;

        assert_eq!(created_notification, expected_notification);

        let response = patch_notification_response(
            &app.app_address,
            created_notification.id,
            &NotificationPatch {
                status: Some(NotificationStatus::Deleted),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(response.status(), 500);

        let body = response.text().await.expect("Cannot get response body");
        assert_eq!(
            body,
            json!({ "message": format!("Failed to delete Todoist task `1234`") }).to_string()
        );
        todoist_mark_thread_as_read_mock.assert();

        let notification = get_notification(&app.app_address, created_notification.id).await;
        assert_eq!(notification.status, NotificationStatus::Unread);
    }
}
