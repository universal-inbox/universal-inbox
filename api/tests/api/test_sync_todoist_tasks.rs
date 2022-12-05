use chrono::{TimeZone, Utc};
use pretty_assertions::assert_eq;
use rstest::*;

use universal_inbox::task::{
    integrations::todoist::{self, TodoistItem, TodoistProject},
    Task, TaskMetadata, TaskPriority, TaskStatus,
};
use universal_inbox_api::universal_inbox::task::source::TaskSourceKind;

use crate::helpers::{
    rest::{create_resource, get_resource},
    task::{
        sync_tasks,
        todoist::{
            assert_sync_items, create_task_from_todoist_item, mock_todoist_sync_items_service,
            mock_todoist_sync_projects_service, sync_todoist_items, sync_todoist_projects,
        },
    },
    tested_app, TestedApp,
};

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_add_new_task_and_update_existing_one(
    #[future] tested_app: TestedApp,
    // Vec[TodoistItem { source_id: "123", ... }, TodoistItem { source_id: "456", ... } ]
    sync_todoist_items: Vec<TodoistItem>,
    sync_todoist_projects: Vec<TodoistProject>,
) {
    let app = tested_app.await;
    let existing_todoist_task = create_resource(
        &app.app_address,
        "tasks",
        Box::new(Task {
            id: uuid::Uuid::new_v4(),
            source_id: sync_todoist_items[1].id.clone(),
            title: "old task 1".to_string(),
            body: "more details".to_string(),
            status: TaskStatus::Active,
            completed_at: None,
            priority: TaskPriority::P4,
            due_at: None,
            source_html_url: todoist::get_task_html_url(&sync_todoist_items[1].id),
            tags: vec!["tag1".to_string()],
            parent_id: None,
            project: "Inbox".to_string(),
            is_recurring: false,
            created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(sync_todoist_items[1].clone()),
        }),
    )
    .await;

    let todoist_tasks_mock =
        mock_todoist_sync_items_service(&app.todoist_mock_server, &sync_todoist_items);
    let todoist_projects_mock =
        mock_todoist_sync_projects_service(&app.todoist_mock_server, &sync_todoist_projects);

    let tasks: Vec<Task> = sync_tasks(&app.app_address, Some(TaskSourceKind::Todoist)).await;

    assert_eq!(tasks.len(), sync_todoist_items.len());
    assert_sync_items(&tasks, &sync_todoist_items);
    todoist_tasks_mock.assert();
    todoist_projects_mock.assert();

    let updated_todoist_task: Box<Task> =
        get_resource(&app.app_address, "tasks", existing_todoist_task.id).await;
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
        TaskMetadata::Todoist(sync_todoist_items[1].clone())
    );
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_mark_as_completed_tasks_not_active_anymore(
    #[future] tested_app: TestedApp,
    // Vec[TodoistTask { source_id: "123", ... }, TodoistTask { source_id: "456", ... } ]
    sync_todoist_items: Vec<TodoistItem>,
    sync_todoist_projects: Vec<TodoistProject>,
) {
    let app = tested_app.await;
    for todoist_item in sync_todoist_items.iter() {
        create_task_from_todoist_item(&app.app_address, todoist_item).await;
    }
    // to be marked as completed during sync
    let existing_todoist_active_task = create_resource(
        &app.app_address,
        "tasks",
        Box::new(Task {
            id: uuid::Uuid::new_v4(),
            source_id: "1789".to_string(),
            title: "Task 3".to_string(),
            body: "".to_string(),
            status: TaskStatus::Active,
            completed_at: None,
            priority: TaskPriority::P4,
            due_at: None,
            source_html_url: todoist::get_task_html_url(&sync_todoist_items[1].id),
            tags: vec![],
            parent_id: None,
            project: "Inbox".to_string(),
            is_recurring: false,
            created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(sync_todoist_items[1].clone()),
        }),
    )
    .await;

    let todoist_sync_items_mock =
        mock_todoist_sync_items_service(&app.todoist_mock_server, &sync_todoist_items);
    let todoist_projects_mock =
        mock_todoist_sync_projects_service(&app.todoist_mock_server, &sync_todoist_projects);

    let tasks: Vec<Task> = sync_tasks(&app.app_address, Some(TaskSourceKind::Todoist)).await;

    assert_eq!(tasks.len(), sync_todoist_items.len());
    assert_sync_items(&tasks, &sync_todoist_items);
    todoist_sync_items_mock.assert();
    todoist_projects_mock.assert();

    let completed_task: Box<Task> =
        get_resource(&app.app_address, "tasks", existing_todoist_active_task.id).await;
    assert_eq!(completed_task.id, existing_todoist_active_task.id);
    assert_eq!(completed_task.status, TaskStatus::Done);
    assert_eq!(completed_task.completed_at.is_some(), true);
}
