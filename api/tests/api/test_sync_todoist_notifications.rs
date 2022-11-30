use chrono::{TimeZone, Utc};
use rstest::*;

use crate::helpers::{
    create_notification, get_notification, sync_notifications, tested_app,
    todoist::{assert_sync_notifications, mock_todoist_tasks_service, sync_todoist_tasks},
    TestedApp,
};
use universal_inbox::{
    integrations::todoist::TodoistTask, Notification, NotificationMetadata, NotificationStatus,
};
use universal_inbox_api::universal_inbox::notification::source::NotificationSourceKind;

async fn create_notification_from_todoist_task(
    app_address: &str,
    todoist_task: &TodoistTask,
) -> Box<Notification> {
    create_notification(
        app_address,
        Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: todoist_task.content.clone(),
            source_id: todoist_task.id.clone(),
            source_html_url: Some(todoist_task.url.clone()),
            status: NotificationStatus::Unread,
            metadata: NotificationMetadata::Todoist(todoist_task.clone()),
            updated_at: todoist_task.created_at,
            last_read_at: None,
            snoozed_until: None,
        }),
    )
    .await
}

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_add_new_notification_and_update_existing_one(
    #[future] tested_app: TestedApp,
    // Vec[TodoistTask { source_id: "123", ... }, TodoistTask { source_id: "456", ... } ]
    sync_todoist_tasks: Vec<TodoistTask>,
) {
    let app = tested_app.await;
    let existing_todoist_notification = create_notification(
        &app.app_address,
        Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "Other task".to_string(),
            status: NotificationStatus::Unread,
            source_id: "1456".to_string(),
            source_html_url: Some(sync_todoist_tasks[1].url.clone()),
            metadata: NotificationMetadata::Todoist(sync_todoist_tasks[1].clone()),
            updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
        }),
    )
    .await;

    let todoist_tasks_mock =
        mock_todoist_tasks_service(&app.todoist_mock_server, &sync_todoist_tasks);

    let notifications: Vec<Notification> =
        sync_notifications(&app.app_address, Some(NotificationSourceKind::Todoist)).await;

    assert_eq!(notifications.len(), sync_todoist_tasks.len());
    assert_sync_notifications(&notifications, &sync_todoist_tasks);
    todoist_tasks_mock.assert();

    let updated_todoist_notification =
        get_notification(&app.app_address, existing_todoist_notification.id).await;
    assert_eq!(
        updated_todoist_notification.id,
        existing_todoist_notification.id
    );
    assert_eq!(
        updated_todoist_notification.source_id,
        existing_todoist_notification.source_id
    );
    assert_eq!(
        updated_todoist_notification.status,
        NotificationStatus::Unread
    );
    assert_eq!(
        updated_todoist_notification.updated_at,
        Utc.with_ymd_and_hms(2019, 12, 11, 22, 37, 50).unwrap()
    );
    assert_eq!(updated_todoist_notification.last_read_at, None);
    assert_eq!(
        updated_todoist_notification.metadata,
        NotificationMetadata::Todoist(sync_todoist_tasks[1].clone())
    );
}

#[rstest]
#[tokio::test]
async fn test_sync_notifications_should_mark_deleted_notification_for_task_not_in_the_inbox_anymore(
    #[future] tested_app: TestedApp,
    // Vec[TodoistTask { source_id: "123", ... }, TodoistTask { source_id: "456", ... } ]
    sync_todoist_tasks: Vec<TodoistTask>,
) {
    let app = tested_app.await;
    for todoist_task in sync_todoist_tasks.iter() {
        create_notification_from_todoist_task(&app.app_address, todoist_task).await;
    }
    // to be deleted during sync
    let existing_todoist_notification = create_notification(
        &app.app_address,
        Box::new(Notification {
            id: uuid::Uuid::new_v4(),
            title: "Task 3".to_string(),
            status: NotificationStatus::Unread,
            source_id: "789".to_string(),
            source_html_url: Some(sync_todoist_tasks[1].url.clone()),
            metadata: NotificationMetadata::Todoist(sync_todoist_tasks[1].clone()),
            updated_at: Utc.with_ymd_and_hms(2014, 11, 6, 0, 0, 0).unwrap(),
            last_read_at: None,
            snoozed_until: None,
        }),
    )
    .await;

    let todoist_tasks_mock =
        mock_todoist_tasks_service(&app.todoist_mock_server, &sync_todoist_tasks);

    let notifications: Vec<Notification> =
        sync_notifications(&app.app_address, Some(NotificationSourceKind::Todoist)).await;

    assert_eq!(notifications.len(), sync_todoist_tasks.len());
    assert_sync_notifications(&notifications, &sync_todoist_tasks);
    todoist_tasks_mock.assert();

    let deleted_notification =
        get_notification(&app.app_address, existing_todoist_notification.id).await;
    assert_eq!(deleted_notification.id, existing_todoist_notification.id);
    assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
}
