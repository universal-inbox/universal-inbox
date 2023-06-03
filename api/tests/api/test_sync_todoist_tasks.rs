use actix_http::StatusCode;
use chrono::{TimeZone, Utc};
use pretty_assertions::assert_eq;
use rstest::*;
use tokio::time::{sleep, Duration};
use tracing::debug;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::IntegrationProviderKind,
    notification::{Notification, NotificationStatus},
    task::{integrations::todoist, Task, TaskMetadata, TaskPriority, TaskStatus},
};
use universal_inbox_api::{
    configuration::Settings,
    integrations::{oauth2::NangoConnection, task::TaskSourceKind, todoist::TodoistSyncResponse},
    universal_inbox::task::TaskCreationResult,
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_and_mock_integration_connection, get_integration_connection_per_provider,
        nango_todoist_connection,
    },
    notification::list_notifications_with_tasks,
    rest::{create_resource, get_resource},
    settings,
    task::{
        list_tasks, sync_tasks, sync_tasks_response,
        todoist::{
            assert_sync_items, create_task_from_todoist_item, mock_todoist_sync_resources_service,
            sync_todoist_items_response, sync_todoist_projects_response,
        },
    },
};

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_add_new_task_and_update_existing_one(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    sync_todoist_items_response: TodoistSyncResponse,
    sync_todoist_projects_response: TodoistSyncResponse,
    nango_todoist_connection: Box<NangoConnection>,
) {
    // existing task will be updated
    // the associated notification will be marked as deleted as the task's project will not be Inbox anymore
    // a new task will be created
    // with an associated notification as the new task's project is Inbox
    let app = authenticated_app.await;
    let todoist_items = sync_todoist_items_response.items.clone().unwrap();
    let existing_todoist_task_creation: Box<TaskCreationResult> = create_resource(
        &app.client,
        &app.app_address,
        "tasks",
        Box::new(Task {
            id: Uuid::new_v4().into(),
            source_id: todoist_items[1].id.clone(),
            title: "old task 1".to_string(),
            body: "more details".to_string(),
            status: TaskStatus::Active,
            completed_at: None,
            priority: TaskPriority::P1,
            due_at: None,
            source_html_url: todoist::get_task_html_url(&todoist_items[1].id),
            tags: vec!["tag1".to_string()],
            parent_id: None,
            project: "Inbox".to_string(),
            is_recurring: false,
            created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(todoist_items[1].clone()),
            user_id: app.user.id,
        }),
    )
    .await;
    let existing_todoist_task = existing_todoist_task_creation.task;
    let existing_todoist_notification = existing_todoist_task_creation.notification.unwrap();
    create_and_mock_integration_connection(
        &app,
        IntegrationProviderKind::Todoist,
        &settings,
        nango_todoist_connection,
    )
    .await;

    let todoist_tasks_mock = mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "items",
        &sync_todoist_items_response,
    );
    let todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
    );

    let task_creations: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(task_creations.len(), todoist_items.len());
    assert_sync_items(&task_creations, &todoist_items, app.user.id);
    todoist_tasks_mock.assert();
    todoist_projects_mock.assert();

    let updated_todoist_task: Box<Task> = get_resource(
        &app.client,
        &app.app_address,
        "tasks",
        existing_todoist_task.id.into(),
    )
    .await;
    assert_eq!(updated_todoist_task.id, existing_todoist_task.id);
    assert_eq!(
        updated_todoist_task.source_id,
        existing_todoist_task.source_id
    );
    // Updated fields
    assert_eq!(updated_todoist_task.title, "Task 2");
    assert_eq!(updated_todoist_task.body, "");
    assert_eq!(updated_todoist_task.priority, TaskPriority::P4);
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
        &app.client,
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
    let notifications = list_notifications_with_tasks(
        &app.client,
        &app.app_address,
        NotificationStatus::Unread,
        false,
        Some(new_task.id),
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].source_id, new_task.source_id);
    assert_eq!(notifications[0].task, Some(new_task.clone()));
    assert_eq!(
        Some(notifications[0].clone().into()),
        new_todoist_task_creation.notification
    );
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_mark_as_completed_tasks_not_active_anymore(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    // Vec[TodoistTask { source_id: "123", ... }, TodoistTask { source_id: "456", ... } ]
    sync_todoist_items_response: TodoistSyncResponse,
    sync_todoist_projects_response: TodoistSyncResponse,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let todoist_items = sync_todoist_items_response.items.clone().unwrap();
    for todoist_item in todoist_items.iter() {
        create_task_from_todoist_item(
            &app.client,
            &app.app_address,
            todoist_item,
            "Inbox".to_string(),
            app.user.id,
        )
        .await;
    }
    // to be marked as completed during sync
    let existing_todoist_active_task_creation: Box<TaskCreationResult> = create_resource(
        &app.client,
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
            user_id: app.user.id,
        }),
    )
    .await;
    let existing_todoist_active_task = existing_todoist_active_task_creation.task;
    let existing_todoist_unread_notification =
        existing_todoist_active_task_creation.notification.unwrap();
    create_and_mock_integration_connection(
        &app,
        IntegrationProviderKind::Todoist,
        &settings,
        nango_todoist_connection,
    )
    .await;

    let todoist_sync_items_mock = mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "items",
        &sync_todoist_items_response,
    );
    let todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
    );

    let task_creations: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(task_creations.len(), todoist_items.len());
    assert_sync_items(&task_creations, &todoist_items, app.user.id);
    todoist_sync_items_mock.assert();
    todoist_projects_mock.assert();

    let completed_task: Box<Task> = get_resource(
        &app.client,
        &app.app_address,
        "tasks",
        existing_todoist_active_task.id.into(),
    )
    .await;
    assert_eq!(completed_task.id, existing_todoist_active_task.id);
    assert_eq!(completed_task.status, TaskStatus::Done);
    assert_eq!(completed_task.completed_at.is_some(), true);

    let deleted_notification: Box<Notification> = get_resource(
        &app.client,
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

#[rstest]
#[tokio::test]
async fn test_sync_all_tasks_asynchronously(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    sync_todoist_items_response: TodoistSyncResponse,
    sync_todoist_projects_response: TodoistSyncResponse,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let todoist_items = sync_todoist_items_response.items.clone().unwrap();
    let _existing_todoist_task_creation: Box<TaskCreationResult> = create_resource(
        &app.client,
        &app.app_address,
        "tasks",
        Box::new(Task {
            id: Uuid::new_v4().into(),
            source_id: todoist_items[1].id.clone(),
            title: "old task 1".to_string(),
            body: "more details".to_string(),
            status: TaskStatus::Active,
            completed_at: None,
            priority: TaskPriority::P1,
            due_at: None,
            source_html_url: todoist::get_task_html_url(&todoist_items[1].id),
            tags: vec!["tag1".to_string()],
            parent_id: None,
            project: "Inbox".to_string(),
            is_recurring: false,
            created_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            metadata: TaskMetadata::Todoist(todoist_items[1].clone()),
            user_id: app.user.id,
        }),
    )
    .await;
    create_and_mock_integration_connection(
        &app,
        IntegrationProviderKind::Todoist,
        &settings,
        nango_todoist_connection,
    )
    .await;

    let mut todoist_tasks_mock = mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "items",
        &sync_todoist_items_response,
    );
    let mut todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
    );

    let unauthenticated_client = reqwest::Client::new();
    let response = sync_tasks_response(
        &unauthenticated_client,
        &app.app_address,
        Some(TaskSourceKind::Todoist),
        true, // asynchronously
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    let result = list_tasks(&app.client, &app.app_address, TaskStatus::Active).await;

    // The existing task's status should not have been updated to Deleted yet
    assert_eq!(result.len(), 1);

    let mut i = 0;
    let synchronized = loop {
        let result = list_tasks(&app.client, &app.app_address, TaskStatus::Active).await;

        debug!("result: {:?}", result);
        if result.len() == 2 {
            // The existing task's status has been updated to Deleted
            break true;
        }

        if i == 10 {
            // Give up after 10 attempts
            break false;
        }

        sleep(Duration::from_millis(100)).await;
        i += 1;
    };

    assert!(synchronized);
    todoist_tasks_mock.assert();
    todoist_projects_mock.assert();

    todoist_tasks_mock.delete();
    todoist_projects_mock.delete();

    // Triggering a new sync should not actually sync again
    let todoist_mock = app.todoist_mock_server.mock(|when, then| {
        when.any_request();
        then.status(200);
    });

    let unauthenticated_client = reqwest::Client::new();
    let response = sync_tasks_response(
        &unauthenticated_client,
        &app.app_address,
        Some(TaskSourceKind::Todoist),
        true, // asynchronously
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    sleep(Duration::from_millis(1000)).await;

    let result = list_tasks(&app.client, &app.app_address, TaskStatus::Active).await;

    // Even after 1s, the existing task's status should not have been updated
    // because the sync happen too soon after the previous one
    assert_eq!(result.len(), 2);
    todoist_mock.assert_hits(0);
}

#[rstest]
#[tokio::test]
async fn test_sync_all_tasks_asynchronously_in_error(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app,
        IntegrationProviderKind::Todoist,
        &settings,
        nango_todoist_connection,
    )
    .await;

    let todoist_mock = app.todoist_mock_server.mock(|when, then| {
        when.any_request();
        then.status(500);
    });

    let unauthenticated_client = reqwest::Client::new();
    let response = sync_tasks_response(
        &unauthenticated_client,
        &app.app_address,
        Some(TaskSourceKind::Todoist),
        true, // asynchronously
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    sleep(Duration::from_millis(1000)).await;

    let result = list_tasks(&app.client, &app.app_address, TaskStatus::Active).await;

    // Even after 1s, the existing task's status should not have been updated
    // because the sync was in error
    assert_eq!(result.len(), 0);
    todoist_mock.assert_hits(1);

    let integration_connection = get_integration_connection_per_provider(
        &app,
        app.user.id,
        IntegrationProviderKind::Todoist,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        integration_connection
            .last_sync_failure_message
            .unwrap()
            .as_str(),
        "Failed to fetch tasks from Todoist"
    );
}
