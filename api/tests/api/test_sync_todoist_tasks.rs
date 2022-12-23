use chrono::{TimeZone, Utc};
use pretty_assertions::assert_eq;
use rstest::*;
use uuid::Uuid;

use universal_inbox::{
    notification::{Notification, NotificationStatus},
    task::{integrations::todoist, Task, TaskMetadata, TaskPriority, TaskStatus},
};
use universal_inbox_api::{
    integrations::{task::TaskSourceKind, todoist::TodoistSyncResponse},
    universal_inbox::task::TaskCreationResult,
};

use crate::helpers::{
    notification::list_notifications,
    rest::{create_resource, get_resource},
    task::{
        sync_tasks,
        todoist::{
            assert_sync_items, create_task_from_todoist_item, mock_todoist_sync_items_service,
            mock_todoist_sync_projects_service, sync_todoist_items_response,
            sync_todoist_projects_response,
        },
    },
    tested_app, TestedApp,
};

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_add_new_task_and_update_existing_one(
    #[future] tested_app: TestedApp,
    sync_todoist_items_response: TodoistSyncResponse,
    sync_todoist_projects_response: TodoistSyncResponse,
) {
    // existing task will be updated
    // the associated notification will be marked as deleted as the task's project will not be Inbox anymore
    // a new task will be created
    // with an associated notification as the new task's project is Inbox
    let app = tested_app.await;
    let todoist_items = sync_todoist_items_response.items.clone().unwrap();
    let existing_todoist_task_creation: Box<TaskCreationResult> = create_resource(
        &app.app_address,
        "tasks",
        Box::new(Task {
            id: Uuid::new_v4().into(),
            source_id: todoist_items[1].id.clone(),
            title: "old task 1".to_string(),
            body: "more details".to_string(),
            status: TaskStatus::Active,
            completed_at: None,
            priority: TaskPriority::P4,
            due_at: None,
            source_html_url: todoist::get_task_html_url(&todoist_items[1].id),
            tags: vec!["tag1".to_string()],
            parent_id: None,
            project: "Inbox".to_string(),
            is_recurring: false,
            created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(todoist_items[1].clone()),
        }),
    )
    .await;
    let existing_todoist_task = existing_todoist_task_creation.task;
    let existing_todoist_notification = existing_todoist_task_creation.notification.unwrap();

    let todoist_tasks_mock =
        mock_todoist_sync_items_service(&app.todoist_mock_server, &sync_todoist_items_response);
    let todoist_projects_mock = mock_todoist_sync_projects_service(
        &app.todoist_mock_server,
        &sync_todoist_projects_response,
    );

    let task_creations: Vec<TaskCreationResult> =
        sync_tasks(&app.app_address, Some(TaskSourceKind::Todoist)).await;

    assert_eq!(task_creations.len(), todoist_items.len());
    assert_sync_items(&task_creations, &todoist_items);
    todoist_tasks_mock.assert();
    todoist_projects_mock.assert();

    let updated_todoist_task: Box<Task> =
        get_resource(&app.app_address, "tasks", existing_todoist_task.id.into()).await;
    assert_eq!(updated_todoist_task.id, existing_todoist_task.id);
    assert_eq!(
        updated_todoist_task.source_id,
        existing_todoist_task.source_id
    );
    // Updated fields
    assert_eq!(updated_todoist_task.title, "Task 2");
    assert_eq!(updated_todoist_task.body, "");
    assert_eq!(updated_todoist_task.priority, TaskPriority::P1);
    assert_eq!(updated_todoist_task.tags.is_empty(), true);
    assert_eq!(updated_todoist_task.project, "Project2".to_string());
    assert_eq!(
        updated_todoist_task.created_at,
        Utc.with_ymd_and_hms(2019, 12, 11, 22, 37, 50).unwrap()
    );
    assert_eq!(
        updated_todoist_task.metadata,
        TaskMetadata::Todoist(todoist_items[1].clone())
    );

    let updated_todoist_notification: Box<Notification> = get_resource(
        &app.app_address,
        "notifications",
        existing_todoist_notification.id.into(),
    )
    .await;
    assert_eq!(
        updated_todoist_notification.id,
        existing_todoist_notification.id
    );
    assert_eq!(
        updated_todoist_notification.source_id,
        existing_todoist_notification.source_id
    );
    assert_eq!(
        updated_todoist_notification.task_id,
        Some(updated_todoist_task.id)
    );
    // The existing notification will be marked as updated but other fields will not be updated
    assert_eq!(
        updated_todoist_notification.status,
        NotificationStatus::Deleted
    );
    assert_eq!(updated_todoist_notification.title, "old task 1");
    assert_eq!(
        updated_todoist_notification.updated_at,
        Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap()
    );

    // Newly created task is not in the Inbox, thus no notification should have been created
    let new_todoist_task_creation = task_creations
        .iter()
        .find(|task_creation| task_creation.task.source_id == todoist_items[0].id)
        .unwrap();
    let new_task = &new_todoist_task_creation.task;
    let notifications_for_new_task = list_notifications(
        &app.app_address,
        NotificationStatus::Unread,
        false,
        Some(new_todoist_task_creation.task.id),
    )
    .await;

    assert_eq!(notifications_for_new_task.len(), 1);
    let new_notification = &notifications_for_new_task[0];
    assert_eq!(new_notification.source_id, new_task.source_id);
    assert_eq!(new_notification.task_id, Some(new_task.id));
    assert_eq!(
        Some(new_notification.clone()),
        new_todoist_task_creation.notification
    );
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_mark_as_completed_tasks_not_active_anymore(
    #[future] tested_app: TestedApp,
    // Vec[TodoistTask { source_id: "123", ... }, TodoistTask { source_id: "456", ... } ]
    sync_todoist_items_response: TodoistSyncResponse,
    sync_todoist_projects_response: TodoistSyncResponse,
) {
    let app = tested_app.await;
    let todoist_items = sync_todoist_items_response.items.clone().unwrap();
    for todoist_item in todoist_items.iter() {
        create_task_from_todoist_item(&app.app_address, todoist_item).await;
    }
    // to be marked as completed during sync
    let existing_todoist_active_task_creation: Box<TaskCreationResult> = create_resource(
        &app.app_address,
        "tasks",
        Box::new(Task {
            id: Uuid::new_v4().into(),
            source_id: "1789".to_string(),
            title: "Task 3".to_string(),
            body: "".to_string(),
            status: TaskStatus::Active,
            completed_at: None,
            priority: TaskPriority::P4,
            due_at: None,
            source_html_url: todoist::get_task_html_url(&todoist_items[1].id),
            tags: vec![],
            parent_id: None,
            project: "Inbox".to_string(),
            is_recurring: false,
            created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(todoist_items[1].clone()),
        }),
    )
    .await;
    let existing_todoist_active_task = existing_todoist_active_task_creation.task;
    let existing_todoist_unread_notification =
        existing_todoist_active_task_creation.notification.unwrap();

    let todoist_sync_items_mock =
        mock_todoist_sync_items_service(&app.todoist_mock_server, &sync_todoist_items_response);
    let todoist_projects_mock = mock_todoist_sync_projects_service(
        &app.todoist_mock_server,
        &sync_todoist_projects_response,
    );

    let task_creations: Vec<TaskCreationResult> =
        sync_tasks(&app.app_address, Some(TaskSourceKind::Todoist)).await;

    assert_eq!(task_creations.len(), todoist_items.len());
    assert_sync_items(&task_creations, &todoist_items);
    todoist_sync_items_mock.assert();
    todoist_projects_mock.assert();

    let completed_task: Box<Task> = get_resource(
        &app.app_address,
        "tasks",
        existing_todoist_active_task.id.into(),
    )
    .await;
    assert_eq!(completed_task.id, existing_todoist_active_task.id);
    assert_eq!(completed_task.status, TaskStatus::Done);
    assert_eq!(completed_task.completed_at.is_some(), true);

    let deleted_notification: Box<Notification> = get_resource(
        &app.app_address,
        "notifications",
        existing_todoist_unread_notification.id.into(),
    )
    .await;
    assert_eq!(
        deleted_notification.task_id,
        Some(existing_todoist_active_task.id)
    );
    assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
}
