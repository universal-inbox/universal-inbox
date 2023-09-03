use std::collections::HashMap;

use actix_http::StatusCode;
use chrono::{TimeZone, Utc};
use pretty_assertions::assert_eq;
use rstest::*;
use tokio::time::{sleep, Duration};
use tracing::debug;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionContext, IntegrationProviderKind, SyncToken,
    },
    notification::{Notification, NotificationStatus},
    task::{
        integrations::todoist::{self, TodoistItem},
        Task, TaskMetadata, TaskPriority, TaskSourceKind, TaskStatus,
    },
};
use universal_inbox_api::{
    configuration::Settings,
    integrations::{oauth2::NangoConnection, todoist::TodoistSyncResponse},
    universal_inbox::task::TaskCreationResult,
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_and_mock_integration_connection, get_integration_connection,
        get_integration_connection_per_provider, nango_todoist_connection,
        update_integration_connection_context,
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
    // the linked notification will be marked as deleted as the task's project will not be Inbox anymore
    // a new task will be created
    // with an linked notification as the new task's project is Inbox
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
    let integration_connection = create_and_mock_integration_connection(
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
        None,
    );
    let todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
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
    // The existing notification will be marked as deleted but other fields will not be updated
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
        vec![NotificationStatus::Unread],
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

    let updated_integration_connection =
        get_integration_connection(&app, integration_connection.id)
            .await
            .unwrap();
    assert_eq!(
        updated_integration_connection,
        IntegrationConnection {
            context: Some(IntegrationConnectionContext::Todoist {
                items_sync_token: SyncToken("todoist_sync_items_token".to_string())
            }),
            ..updated_integration_connection.clone()
        }
    );
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_add_new_empty_task(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    nango_todoist_connection: Box<NangoConnection>,
) {
    // Somehow, Todoist may return empty TodoistItems not attached to any project
    let app = authenticated_app.await;
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
        &TodoistSyncResponse {
            items: Some(vec![TodoistItem {
                id: "id1".to_string(),
                parent_id: None,
                project_id: "0".to_string(),
                sync_id: None,
                section_id: None,
                content: "".to_string(),
                description: "".to_string(),
                labels: vec![],
                child_order: 0,
                day_order: Some(-1),
                priority: todoist::TodoistItemPriority::P1,
                checked: false,
                is_deleted: true,
                collapsed: false,
                completed_at: None,
                added_at: Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap(),
                due: None,
                user_id: "11".to_string(),
                added_by_uid: None,
                assigned_by_uid: None,
                responsible_uid: None,
            }]),
            projects: None,
            full_sync: true,
            temp_id_mapping: HashMap::new(),
            sync_token: SyncToken("sync_token".to_string()),
        },
        None,
    );
    let todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "projects",
        &TodoistSyncResponse {
            items: None,
            projects: Some(vec![]),
            full_sync: true,
            temp_id_mapping: HashMap::new(),
            sync_token: SyncToken("project_sync_token".to_string()),
        },
        None,
    );

    let task_creations: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(task_creations.len(), 1);
    assert!(task_creations[0].notification.is_none());
    assert_eq!(task_creations[0].task.body, "".to_string());
    assert!(task_creations[0].task.completed_at.is_none());
    assert!(task_creations[0].task.due_at.is_none());
    assert!(!task_creations[0].task.is_recurring);
    assert!(task_creations[0].task.parent_id.is_none());
    assert_eq!(task_creations[0].task.priority, TaskPriority::P4);
    assert_eq!(task_creations[0].task.project, "No project".to_string());
    assert_eq!(task_creations[0].task.source_id, "id1".to_string());
    assert_eq!(task_creations[0].task.status, TaskStatus::Deleted);
    assert!(task_creations[0].task.tags.is_empty());
    assert_eq!(task_creations[0].task.title, "".to_string());
    //assert_sync_items(&task_creations, &todoist_items, app.user.id);
    todoist_tasks_mock.assert();
    todoist_projects_mock.assert();
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_reuse_existing_sync_token(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let integration_connection = create_and_mock_integration_connection(
        &app,
        IntegrationProviderKind::Todoist,
        &settings,
        nango_todoist_connection,
    )
    .await;

    let sync_token = SyncToken("previous_sync_token".to_string());
    update_integration_connection_context(
        &app,
        integration_connection.id,
        IntegrationConnectionContext::Todoist {
            items_sync_token: sync_token.clone(),
        },
    )
    .await;

    let sync_todoist_items_response = TodoistSyncResponse {
        items: Some(vec![]),
        projects: None,
        full_sync: false,
        temp_id_mapping: HashMap::new(),
        sync_token: SyncToken("new_sync_token".to_string()),
    };
    let todoist_tasks_mock = mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "items",
        &sync_todoist_items_response,
        Some(sync_token),
    );

    let task_creations: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(task_creations.len(), 0);
    todoist_tasks_mock.assert();

    let updated_integration_connection =
        get_integration_connection(&app, integration_connection.id)
            .await
            .unwrap();
    assert_eq!(
        updated_integration_connection,
        IntegrationConnection {
            context: Some(IntegrationConnectionContext::Todoist {
                items_sync_token: SyncToken("new_sync_token".to_string())
            }),
            ..updated_integration_connection.clone()
        },
    );
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_mark_as_completed_tasks_not_active_anymore(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    mut sync_todoist_items_response: TodoistSyncResponse,
    sync_todoist_projects_response: TodoistSyncResponse,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    // Only task "1123" will be marked as Done during the sync, keeping reference to task and
    // notifications ids
    let mut task_creation: Option<TaskCreationResult> = None;
    {
        let todoist_items = sync_todoist_items_response.items.as_mut().unwrap();
        for todoist_item in todoist_items.iter() {
            let creation = create_task_from_todoist_item(
                &app.client,
                &app.app_address,
                todoist_item,
                "Inbox".to_string(),
                app.user.id,
            )
            .await;
            if creation.task.source_id == "1123" {
                task_creation = Some(*creation);
            }
        }
        // Mark task `1123` as completed in the sync response
        let mut task: &mut TodoistItem = todoist_items.iter_mut().find(|i| i.id == "1123").unwrap();
        task.checked = true;
        task.completed_at = Some(Utc::now());
    }
    let task_creation = task_creation.unwrap();
    let todoist_items = sync_todoist_items_response.items.clone().unwrap();

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
        None,
    );
    let todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
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
        task_creation.task.id.into(),
    )
    .await;
    assert_eq!(completed_task.id, task_creation.task.id);
    assert_eq!(completed_task.status, TaskStatus::Done);
    assert_eq!(completed_task.completed_at.is_some(), true);

    let deleted_notification: Box<Notification> = get_resource(
        &app.client,
        &app.app_address,
        "notifications",
        task_creation.notification.unwrap().id.into(),
    )
    .await;
    assert_eq!(deleted_notification.task_id, Some(task_creation.task.id));
    assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_not_update_tasks_and_notifications_with_empty_incremental_sync_response(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    mut sync_todoist_items_response: TodoistSyncResponse,
    sync_todoist_projects_response: TodoistSyncResponse,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let mut tasks_created: Vec<TaskCreationResult> = vec![];
    {
        let todoist_items = sync_todoist_items_response.items.as_ref().unwrap();
        for todoist_item in todoist_items.iter() {
            let creation = create_task_from_todoist_item(
                &app.client,
                &app.app_address,
                todoist_item,
                "Inbox".to_string(),
                app.user.id,
            )
            .await;
            tasks_created.push(*creation);
        }
    }
    // Create a response with only the task 1456 in it.
    // Task 1123 and its notification should not be updated
    sync_todoist_items_response.items = Some(
        sync_todoist_items_response
            .items
            .unwrap()
            .into_iter()
            .skip(1)
            .collect(),
    );

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
        None,
    );
    let todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    );

    let task_creations: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(task_creations.len(), 1);
    assert_eq!(
        task_creations[0].task.source_id,
        sync_todoist_items_response.items.unwrap()[0].id
    );
    assert_eq!(task_creations[0].task.status, TaskStatus::Active);
    assert!(task_creations[0].notification.is_none());
    todoist_sync_items_mock.assert();
    todoist_projects_mock.assert();

    // We want to assert that the task that was not in the response (ie. source = "1123") has not
    // been updated
    for task_creation in tasks_created
        .iter()
        .filter(|tc| tc.task.source_id != task_creations[0].task.source_id)
    {
        let task: Box<Task> = get_resource(
            &app.client,
            &app.app_address,
            "tasks",
            task_creation.task.id.into(),
        )
        .await;
        assert_eq!(task.id, task_creation.task.id);
        assert_eq!(task.status, TaskStatus::Active);
        assert!(task.completed_at.is_none());

        let notification: Box<Notification> = get_resource(
            &app.client,
            &app.app_address,
            "notifications",
            task_creation.notification.as_ref().unwrap().id.into(),
        )
        .await;
        assert_eq!(notification.task_id, Some(task_creation.task.id));
        assert_eq!(notification.status, NotificationStatus::Unread);
    }
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
        None,
    );
    let mut todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
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

        debug!("result: {result:?}");
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
