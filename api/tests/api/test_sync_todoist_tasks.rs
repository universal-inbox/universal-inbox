use std::collections::HashMap;

use anyhow::anyhow;
use chrono::{TimeZone, Utc};
use http::StatusCode;
use pretty_assertions::assert_eq;
use rstest::*;
use tokio::time::{Duration, sleep};
use tokio_retry::{Retry, strategy::FixedInterval};

use universal_inbox::{
    integration_connection::{
        IntegrationConnection, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::todoist::{SyncToken, TodoistConfig, TodoistContext},
        provider::{IntegrationConnectionContext, IntegrationProvider, IntegrationProviderKind},
    },
    notification::{Notification, NotificationStatus, NotificationWithTask},
    task::{Task, TaskCreationResult, TaskPriority, TaskSourceKind, TaskStatus},
    third_party::{
        integrations::todoist::{self, TodoistItem},
        item::{ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
};
use universal_inbox_api::{configuration::Settings, integrations::todoist::TodoistSyncResponse};

use crate::helpers::integration_connection::OAuthCredentialFixture;
use crate::helpers::third_party::create_task_third_party_item;
use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, create_integration_connection,
        get_integration_connection, get_integration_connection_per_provider,
        todoist_oauth_credential, update_integration_connection_context,
    },
    notification::list_notifications_with_tasks,
    rest::get_resource,
    settings,
    task::{
        list_tasks, sync_tasks, sync_tasks_response,
        todoist::{
            assert_sync_items, mock_todoist_sync_resources_service, sync_todoist_items_response,
            sync_todoist_projects_response,
        },
    },
};
use wiremock::{Mock, ResponseTemplate};

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_add_new_task_and_update_existing_one(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    sync_todoist_items_response: TodoistSyncResponse,
    sync_todoist_projects_response: TodoistSyncResponse,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    // existing task will be updated
    // the linked notification will be marked as deleted as the task's project will not be Inbox anymore
    // a new task will be created
    // with an linked notification as the new task's project is Inbox
    let app = authenticated_app.await;
    let integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
    let _todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;

    let todoist_items = sync_todoist_items_response.items.clone().unwrap();
    let existing_todoist_third_party_item_creation = create_task_third_party_item(
        &app.app,
        ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
            content: "old task 1".to_string(),
            description: "more details".to_string(),
            checked: false,
            is_deleted: false,
            completed_at: None,
            priority: todoist::TodoistItemPriority::P4,
            due: None,
            labels: vec![todoist::TodoistLabel::from_name("tag1")],
            parent_id: None,
            project_id: "1111".to_string(), // ie. "Inbox"
            //added_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            ..todoist_items[1].clone()
        })),
        app.user.id,
    )
    .await;
    let existing_todoist_task = existing_todoist_third_party_item_creation
        .task
        .as_ref()
        .unwrap();
    let existing_todoist_notification = existing_todoist_third_party_item_creation
        .notification
        .as_ref()
        .unwrap();

    let _todoist_tasks_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "items",
        &sync_todoist_items_response,
        None,
    )
    .await;

    let task_creations: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(task_creations.len(), todoist_items.len());
    assert_sync_items(&task_creations, &todoist_items, app.user.id);

    let updated_todoist_task: Box<Task> = get_resource(
        &app.client,
        &app.app.api_address,
        "tasks",
        existing_todoist_task.id.into(),
    )
    .await;
    assert_eq!(updated_todoist_task.id, existing_todoist_task.id);
    assert_eq!(
        updated_todoist_task.source_item.source_id,
        existing_todoist_task.source_item.source_id
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
        updated_todoist_task.source_item.data,
        ThirdPartyItemData::TodoistItem(Box::new(todoist_items[1].clone()))
    );

    let updated_todoist_notification: Box<NotificationWithTask> = get_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        existing_todoist_notification.id.into(),
    )
    .await;
    assert_eq!(
        updated_todoist_notification.id,
        existing_todoist_notification.id
    );
    assert_eq!(
        updated_todoist_notification.source_item.source_id,
        existing_todoist_notification.source_item.source_id
    );
    assert_eq!(
        updated_todoist_notification.task.as_ref().map(|t| t.id),
        Some(updated_todoist_task.id)
    );
    // The existing notification will be marked as deleted but other fields will not be updated
    assert_eq!(
        updated_todoist_notification.status,
        NotificationStatus::Deleted
    );
    assert_eq!(updated_todoist_notification.title, "old task 1");

    // Newly created task is in the Inbox, thus a notification should be created
    let new_todoist_task_creation = task_creations
        .iter()
        .find(|task_creation| task_creation.task.source_item.source_id == todoist_items[0].id)
        .unwrap();
    let new_task = &new_todoist_task_creation.task;
    let notifications = list_notifications_with_tasks(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        Some(new_task.id),
        None,
        false,
    )
    .await;

    assert_eq!(notifications.len(), 1);
    assert_eq!(
        notifications[0].source_item.source_id,
        new_task.source_item.source_id
    );
    assert_eq!(notifications[0].task, Some(new_task.clone()));
    assert_eq!(
        Into::<Notification>::into(notifications[0].clone()),
        new_todoist_task_creation.notifications[0]
    );

    let updated_integration_connection =
        get_integration_connection(&app, integration_connection.id)
            .await
            .unwrap();
    assert_eq!(
        updated_integration_connection,
        IntegrationConnection {
            provider: IntegrationProvider::Todoist {
                context: Some(TodoistContext {
                    items_sync_token: SyncToken("todoist_sync_items_token".to_string())
                }),
                config: TodoistConfig::enabled()
            },
            ..updated_integration_connection.clone()
        }
    );

    let integration_connection = get_integration_connection_per_provider(
        &app,
        app.user.id,
        IntegrationProviderKind::Todoist,
        None,
        None,
    )
    .await
    .unwrap();
    assert!(integration_connection.last_tasks_sync_started_at.is_some());
    assert!(
        integration_connection
            .last_tasks_sync_completed_at
            .is_some()
    );
    assert!(integration_connection.last_tasks_sync_failed_at.is_none());
    assert!(
        integration_connection
            .last_tasks_sync_failure_message
            .is_none()
    );
    assert_eq!(integration_connection.tasks_sync_failures, 0);
    assert_eq!(
        integration_connection.status,
        IntegrationConnectionStatus::Validated
    );
    assert!(integration_connection.failure_message.is_none(),);
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_add_new_task_and_delete_notification_when_disabling_notification_creation_from_inbox_tasks(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    sync_todoist_items_response: TodoistSyncResponse,
    sync_todoist_projects_response: TodoistSyncResponse,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    // When disabling the creation of notifications from Inbox tasks, no notification should be created
    let app = authenticated_app.await;
    let todoist_items = sync_todoist_items_response.items.clone().unwrap();
    let _integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig {
            create_notification_from_inbox_task: false,
            ..TodoistConfig::enabled()
        }),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;

    let _todoist_tasks_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "items",
        &sync_todoist_items_response,
        None,
    )
    .await;
    let _todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;

    let task_creations: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(task_creations.len(), todoist_items.len());
    assert!(
        task_creations
            .iter()
            .all(|task_creation| { task_creation.notifications.is_empty() })
    );

    let notifications = list_notifications_with_tasks(
        &app.client,
        &app.app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
        false,
    )
    .await;

    assert_eq!(notifications.len(), 0);
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_add_new_empty_task(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    // Somehow, Todoist may return empty TodoistItems not attached to any project
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;

    let _todoist_tasks_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
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
            labels: None,
            full_sync: true,
            temp_id_mapping: HashMap::new(),
            sync_token: SyncToken("sync_token".to_string()),
        },
        None,
    )
    .await;
    let _todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &TodoistSyncResponse {
            items: None,
            projects: Some(vec![]),
            labels: None,
            full_sync: true,
            temp_id_mapping: HashMap::new(),
            sync_token: SyncToken("project_sync_token".to_string()),
        },
        None,
    )
    .await;

    let task_creations: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(task_creations.len(), 1);
    assert!(task_creations[0].notifications.is_empty());
    assert_eq!(task_creations[0].task.body, "".to_string());
    assert!(task_creations[0].task.completed_at.is_none());
    assert!(task_creations[0].task.due_at.is_none());
    assert!(!task_creations[0].task.is_recurring);
    assert!(task_creations[0].task.parent_id.is_none());
    assert_eq!(task_creations[0].task.priority, TaskPriority::P4);
    assert_eq!(task_creations[0].task.project, "No project".to_string());
    assert_eq!(
        task_creations[0].task.source_item.source_id,
        "id1".to_string()
    );
    assert_eq!(task_creations[0].task.status, TaskStatus::Deleted);
    assert!(task_creations[0].task.tags.is_empty());
    assert_eq!(task_creations[0].task.title, "".to_string());
    //assert_sync_items(&task_creations, &todoist_items, app.user.id);
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_reuse_existing_sync_token(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;

    let sync_token = SyncToken("previous_sync_token".to_string());
    update_integration_connection_context(
        &app,
        integration_connection.id,
        IntegrationConnectionContext::Todoist(TodoistContext {
            items_sync_token: sync_token.clone(),
        }),
    )
    .await;

    let sync_todoist_items_response = TodoistSyncResponse {
        items: Some(vec![]),
        projects: None,
        labels: None,
        full_sync: false,
        temp_id_mapping: HashMap::new(),
        sync_token: SyncToken("new_sync_token".to_string()),
    };
    let _todoist_tasks_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "items",
        &sync_todoist_items_response,
        Some(sync_token),
    )
    .await;

    let task_creations: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(task_creations.len(), 0);

    let updated_integration_connection =
        get_integration_connection(&app, integration_connection.id)
            .await
            .unwrap();
    assert_eq!(
        updated_integration_connection,
        IntegrationConnection {
            provider: IntegrationProvider::Todoist {
                context: Some(TodoistContext {
                    items_sync_token: SyncToken("new_sync_token".to_string())
                }),
                config: TodoistConfig::enabled()
            },
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
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let _integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
    let _todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;

    // Only task "1123" will be marked as Done during the sync, keeping reference to task and
    // notifications ids
    // notification for task "1123" will be marked as deleted because the task is done
    // notification for task "1456" will be marked as deleted because the task is not in the inbox anymore
    let mut third_party_item_creation: Option<ThirdPartyItemCreationResult> = None;
    {
        let todoist_items = sync_todoist_items_response.items.as_mut().unwrap();
        for todoist_item in todoist_items.iter() {
            let creation = create_task_third_party_item(
                &app.app,
                ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                    project_id: "1111".to_string(), // ie. "Inbox"
                    added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..todoist_item.clone()
                })),
                app.user.id,
            )
            .await;
            if creation.third_party_item.source_id == "1123" {
                third_party_item_creation = Some(*creation);
            }
        }
        // Mark task `1123` as completed in the sync response
        let task: &mut TodoistItem = todoist_items.iter_mut().find(|i| i.id == "1123").unwrap();
        task.checked = true;
        task.completed_at = Some(Utc::now());
    }
    let third_party_item_creation = third_party_item_creation.unwrap();
    let todoist_items = sync_todoist_items_response.items.clone().unwrap();

    let _todoist_sync_items_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "items",
        &sync_todoist_items_response,
        None,
    )
    .await;

    let task_creations: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(task_creations.len(), todoist_items.len());
    assert_sync_items(&task_creations, &todoist_items, app.user.id);

    let completed_task: Box<Task> = get_resource(
        &app.client,
        &app.app.api_address,
        "tasks",
        third_party_item_creation.task.as_ref().unwrap().id.into(),
    )
    .await;
    assert_eq!(
        completed_task.id,
        third_party_item_creation.task.as_ref().unwrap().id
    );
    assert_eq!(completed_task.status, TaskStatus::Done);
    assert_eq!(completed_task.completed_at.is_some(), true);

    let deleted_notification: Box<NotificationWithTask> = get_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        third_party_item_creation
            .notification
            .as_ref()
            .unwrap()
            .id
            .into(),
    )
    .await;
    assert_eq!(
        deleted_notification.task.as_ref().map(|t| t.id),
        Some(third_party_item_creation.task.as_ref().unwrap().id)
    );
    assert_eq!(deleted_notification.status, NotificationStatus::Deleted);
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_not_update_tasks_and_notifications_with_empty_incremental_sync_response(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    mut sync_todoist_items_response: TodoistSyncResponse,
    sync_todoist_projects_response: TodoistSyncResponse,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    let _integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
    let _todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;

    let mut third_party_items_created: Vec<ThirdPartyItemCreationResult> = vec![];
    {
        let todoist_items = sync_todoist_items_response.items.as_ref().unwrap();
        for todoist_item in todoist_items.iter() {
            let creation = create_task_third_party_item(
                &app.app,
                ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                    project_id: "1111".to_string(), // ie. "Inbox"
                    //added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
                    ..todoist_item.clone()
                })),
                app.user.id,
            )
            .await;
            third_party_items_created.push(*creation);
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

    let _todoist_sync_items_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "items",
        &sync_todoist_items_response,
        None,
    )
    .await;

    let task_creations: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(task_creations.len(), 1);
    assert_eq!(
        task_creations[0].task.source_item.source_id,
        sync_todoist_items_response.items.unwrap()[0].id
    );
    assert_eq!(task_creations[0].task.status, TaskStatus::Active);
    assert_eq!(task_creations[0].notifications.len(), 1);

    // We want to assert that the task that was not in the response (ie. source = "1123") has not
    // been updated
    for third_party_item_created in third_party_items_created.iter().filter(|tpic| {
        tpic.task.as_ref().unwrap().source_item.source_id
            != task_creations[0].task.source_item.source_id
    }) {
        let existing_task = third_party_item_created.task.as_ref().unwrap();
        let existing_notification = third_party_item_created.notification.as_ref().unwrap();
        let task: Box<Task> = get_resource(
            &app.client,
            &app.app.api_address,
            "tasks",
            existing_task.id.into(),
        )
        .await;
        assert_eq!(task.id, existing_task.id);
        assert_eq!(task.status, TaskStatus::Active);
        assert!(task.completed_at.is_none());

        let notification: Box<NotificationWithTask> = get_resource(
            &app.client,
            &app.app.api_address,
            "notifications",
            existing_notification.id.into(),
        )
        .await;
        assert_eq!(
            notification.task.as_ref().map(|t| t.id),
            Some(existing_task.id)
        );
        assert_eq!(notification.status, NotificationStatus::Unread);
    }
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_with_no_validated_integration_connections(
    #[future] authenticated_app: AuthenticatedApp,
) {
    let app = authenticated_app.await;
    create_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        IntegrationConnectionStatus::Created,
        None,
        None,
        None,
        None,
        None,
    )
    .await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.app.todoist_mock_server)
        .await;

    let response = sync_tasks_response(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_with_synchronization_disabled(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::disabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.app.todoist_mock_server)
        .await;

    let response = sync_tasks_response(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
}

#[rstest]
#[case::trigger_sync_when_listing_tasks(true)]
#[case::trigger_sync_with_sync_endpoint(false)]
#[tokio::test]
async fn test_sync_all_tasks_asynchronously(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    sync_todoist_items_response: TodoistSyncResponse,
    sync_todoist_projects_response: TodoistSyncResponse,
    todoist_oauth_credential: OAuthCredentialFixture,
    #[case] trigger_sync_when_listing_tasks: bool,
) {
    let app = authenticated_app.await;
    let _integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;
    let _todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;

    let todoist_items = sync_todoist_items_response.items.clone().unwrap();
    let _existing_todoist_third_party_item_creation = create_task_third_party_item(
        &app.app,
        ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
            content: "old task 1".to_string(),
            description: "more details".to_string(),
            checked: false,
            is_deleted: false,
            completed_at: None,
            priority: todoist::TodoistItemPriority::P4,
            due: None,
            labels: vec![todoist::TodoistLabel::from_name("tag1")],
            parent_id: None,
            project_id: "1111".to_string(), // ie. "Inbox"
            //added_at: Utc.with_ymd_and_hms(2022, 1, 1, 0, 0, 0).unwrap(),
            ..todoist_items[1].clone()
        })),
        app.user.id,
    )
    .await;

    let _todoist_tasks_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "items",
        &sync_todoist_items_response,
        None,
    )
    .await;

    if trigger_sync_when_listing_tasks {
        let result = list_tasks(&app.client, &app.app.api_address, TaskStatus::Active, true).await;

        // The existing task's status should not have been updated to Deleted yet
        assert_eq!(result.len(), 1);
    } else {
        let response = sync_tasks_response(
            &app.client,
            &app.app.api_address,
            Some(TaskSourceKind::Todoist),
            true, // asynchronously
        )
        .await;

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    Retry::start(FixedInterval::from_millis(100).take(10), || async {
        let result = list_tasks(
            &app.client,
            &app.app.api_address,
            TaskStatus::Active,
            trigger_sync_when_listing_tasks,
        )
        .await;

        if result.len() == 2 {
            Ok(())
        } else {
            Err(anyhow!("Not yet synchronized"))
        }
    })
    .await
    .unwrap();

    // Triggering a new sync should not actually sync again
    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.app.todoist_mock_server)
        .await;
    let response = sync_tasks_response(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Todoist),
        true, // asynchronously
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    sleep(Duration::from_millis(1000)).await;

    let result = list_tasks(&app.client, &app.app.api_address, TaskStatus::Active, false).await;

    // Even after 1s, the existing task's status should not have been updated
    // because the sync happen too soon after the previous one
    assert_eq!(result.len(), 2);
}

#[rstest]
#[tokio::test]
async fn test_sync_all_tasks_asynchronously_in_error(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    todoist_oauth_credential: OAuthCredentialFixture,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        todoist_oauth_credential,
        None,
        None,
    )
    .await;

    Mock::given(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(400))
        .mount(&app.app.todoist_mock_server)
        .await;
    let response = sync_tasks_response(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Todoist),
        true, // asynchronously
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    sleep(Duration::from_millis(1000)).await;

    let result = list_tasks(&app.client, &app.app.api_address, TaskStatus::Active, false).await;

    // Even after 1s, the existing task's status should not have been updated
    // because the sync was in error
    assert_eq!(result.len(), 0);

    let integration_connection = get_integration_connection_per_provider(
        &app,
        app.user.id,
        IntegrationProviderKind::Todoist,
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        integration_connection
            .last_tasks_sync_failure_message
            .unwrap()
            .as_str(),
        "Failed to fetch tasks from Todoist"
    );
}

mod project_contexts {
    use super::*;
    use pretty_assertions::assert_eq;

    use chrono::SubsecRound;
    use sqlx::Row;
    use uuid::Uuid;

    use universal_inbox::{
        audit::{AuditActor, AuditUserChannel},
        context::{Context, ContextBinding, ContextId, ContextStatus, Note, NoteId},
        task::{ProjectId, ProjectSummary},
    };
    use universal_inbox_api::{
        repository::context::ContextRepository,
        universal_inbox::{UpsertStatus, audit::actor_context::with_system_actor},
    };

    use crate::helpers::{TestedApp, set_audit_actor};

    fn actor() -> AuditActor {
        AuditActor::User {
            via: AuditUserChannel::Web,
        }
    }

    async fn list_contexts(app: &AuthenticatedApp) -> Vec<Context> {
        let mut transaction = app.app.repository.begin().await.unwrap();
        app.app
            .repository
            .list_contexts_for_user(&mut transaction, app.user.id)
            .await
            .unwrap()
    }

    async fn note_of(app: &AuthenticatedApp, context: &Context) -> Option<Note> {
        let mut transaction = app.app.repository.begin().await.unwrap();
        app.app
            .repository
            .get_note_for_context(&mut transaction, context.id)
            .await
            .unwrap()
    }

    fn project(source_id: &str, name: &str) -> ProjectSummary {
        ProjectSummary {
            source_id: ProjectId(source_id.to_string()),
            name: name.to_string(),
        }
    }

    /// Upsert `projects` in a transaction of its own, as the sync does
    async fn upsert(
        app: &AuthenticatedApp,
        projects: Vec<ProjectSummary>,
    ) -> Vec<UpsertStatus<Box<Context>>> {
        let mut transaction = app.app.repository.begin().await.unwrap();
        set_audit_actor(&mut transaction, &actor()).await;
        let upserts = app
            .app
            .task_service
            .read()
            .await
            .upsert_mirrored_contexts(
                &mut transaction,
                app.user.id,
                IntegrationProviderKind::Todoist,
                projects,
            )
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        upserts
    }

    async fn context_audit_actions(app: &TestedApp, entity_id: Uuid) -> Vec<String> {
        sqlx::query(
            "SELECT action FROM audit_event
             WHERE entity_id = $1 AND entity_kind = 'Context' ORDER BY created_at, id",
        )
        .bind(entity_id)
        .fetch_all(&*app.repository.pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get("action"))
        .collect()
    }

    async fn connect_todoist(
        app: &AuthenticatedApp,
        settings: &Settings,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        create_and_mock_integration_connection(
            &app.app,
            app.user.id,
            IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
            settings,
            todoist_oauth_credential,
            None,
            None,
        )
        .await;
    }

    #[rstest]
    #[tokio::test]
    async fn test_sync_tasks_mirrors_each_todoist_project_as_one_context(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        sync_todoist_items_response: TodoistSyncResponse,
        sync_todoist_projects_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        connect_todoist(&app, &settings, todoist_oauth_credential).await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "items",
            &sync_todoist_items_response,
            None,
        )
        .await;

        let task_creations: Vec<TaskCreationResult> = sync_tasks(
            &app.client,
            &app.app.api_address,
            Some(TaskSourceKind::Todoist),
            false,
        )
        .await;

        // `Task.project` is still the project name
        let items_count = sync_todoist_items_response.items.as_ref().unwrap().len();
        assert_eq!(task_creations.len(), items_count);
        assert!(
            task_creations
                .iter()
                .all(|creation| ["Inbox", "Project2"].contains(&creation.task.project.as_str()))
        );

        let contexts = list_contexts(&app).await;
        assert_eq!(contexts.len(), 2);
        for (source_id, name) in [("1111", "Inbox"), ("2222", "Project2")] {
            let context = contexts
                .iter()
                .find(|context| context.name == name)
                .unwrap_or_else(|| panic!("no context for {name}"));
            assert_eq!(
                context.binding,
                Some(
                    ContextBinding::new(IntegrationProviderKind::Todoist, source_id.into(), true)
                        .unwrap()
                )
            );
            assert_eq!(context.parent_id, None);
            assert_eq!(context.status, ContextStatus::Active);
            let note = note_of(&app, context).await.expect("a context has a note");
            assert_eq!(note.id, context.note_id);
            assert_eq!(note.body, "");
            // Recorded as made by the sync job
            assert_eq!(
                context_audit_actions(&app.app, context.id.0).await,
                vec!["Created"]
            );
        }
        let actor_kind: String = sqlx::query_scalar(
            "SELECT actor_kind FROM audit_event WHERE entity_kind = 'Context' LIMIT 1",
        )
        .fetch_one(&*app.app.repository.pool)
        .await
        .unwrap();
        assert_eq!(actor_kind, "System");
    }

    #[rstest]
    #[tokio::test]
    async fn test_upsert_renames_the_context_bound_to_the_same_source_id(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let created = upsert(&app, vec![project("2222", "Project2")]).await;
        let UpsertStatus::Created(created) = &created[0] else {
            panic!("expected a creation, got {:?}", created[0]);
        };

        let renamed = upsert(&app, vec![project("2222", "Renamed project")]).await;

        let UpsertStatus::Updated { old, new } = &renamed[0] else {
            panic!("expected an update, got {:?}", renamed[0]);
        };
        assert_eq!(old.name, "Project2");
        assert_eq!(new.name, "Renamed project");
        assert_eq!(new.id, created.id);
        assert_eq!(new.note_id, created.note_id);
        assert_eq!(new.binding, created.binding);
        assert!(new.updated_at > created.updated_at);
        let contexts = list_contexts(&app).await;
        assert_eq!(contexts.len(), 1);
        assert_eq!(contexts[0].name, "Renamed project");
        assert_eq!(
            context_audit_actions(&app.app, created.id.0).await,
            vec!["Created", "Updated"]
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_upsert_of_an_unchanged_project_writes_nothing(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let created = upsert(&app, vec![project("2222", "Project2")]).await;
        let UpsertStatus::Created(created) = &created[0] else {
            panic!("expected a creation, got {:?}", created[0]);
        };
        let before = list_contexts(&app).await;

        let again = upsert(&app, vec![project("2222", "Project2")]).await;

        let UpsertStatus::Untouched(untouched) = &again[0] else {
            panic!("expected no change, got {:?}", again[0]);
        };
        assert_eq!(untouched.id, created.id);
        assert_eq!(list_contexts(&app).await, before);
        assert_eq!(
            context_audit_actions(&app.app, created.id.0).await,
            vec!["Created"]
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_upsert_leaves_a_context_the_ui_owns_alone(
        #[future] authenticated_app: AuthenticatedApp,
    ) {
        let app = authenticated_app.await;
        let now = Utc::now().trunc_subsecs(6);
        let new_context = |name: &str, binding: Option<ContextBinding>| {
            Box::new(Context {
                id: ContextId(Uuid::new_v4()),
                user_id: app.user.id,
                parent_id: None,
                name: name.to_string(),
                binding,
                status: ContextStatus::Active,
                note_id: NoteId(Uuid::new_v4()),
                created_at: now,
                updated_at: now,
            })
        };
        let not_mirrored = new_context(
            "My name for it",
            Some(
                ContextBinding::new(IntegrationProviderKind::Todoist, "2222".into(), false)
                    .unwrap(),
            ),
        );
        let unbound = new_context("Project2", None);
        let mut transaction = app.app.repository.begin().await.unwrap();
        set_audit_actor(&mut transaction, &actor()).await;
        for context in [&not_mirrored, &unbound] {
            app.app
                .repository
                .create_context_with_note(&mut transaction, context.clone(), String::new())
                .await
                .unwrap();
        }
        transaction.commit().await.unwrap();

        // The binding is on the source id: the name is Universal Inbox's, and the unbound
        // context named like the project is not the project's context
        let same_source = upsert(&app, vec![project("2222", "Project2")]).await;
        assert!(
            matches!(&same_source[0], UpsertStatus::Untouched(context) if context.id == not_mirrored.id)
        );
        let other_source = upsert(&app, vec![project("3333", "Project2")]).await;
        let UpsertStatus::Created(created) = &other_source[0] else {
            panic!("expected a creation, got {:?}", other_source[0]);
        };
        assert_ne!(created.id, unbound.id);

        let contexts = list_contexts(&app).await;
        assert_eq!(contexts.len(), 3);
        let stored = |id: ContextId| contexts.iter().find(|context| context.id == id).unwrap();
        assert_eq!(stored(not_mirrored.id).name, "My name for it");
        assert_eq!(stored(unbound.id).binding, None);
    }

    #[rstest]
    #[tokio::test]
    async fn test_deleted_projects_get_no_context_and_archived_ones_an_active_one(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        mut sync_todoist_projects_response: TodoistSyncResponse,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        connect_todoist(&app, &settings, todoist_oauth_credential).await;
        let projects = sync_todoist_projects_response.projects.as_mut().unwrap();
        let mut deleted = projects[1].clone();
        deleted.id = "3333".to_string();
        deleted.name = "Deleted project".to_string();
        deleted.is_deleted = true;
        let mut archived = projects[1].clone();
        archived.id = "4444".to_string();
        archived.name = "Archived project".to_string();
        archived.is_archived = true;
        projects.push(deleted);
        projects.push(archived);
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "projects",
            &sync_todoist_projects_response,
            None,
        )
        .await;

        // As the sync runs it, under the system actor
        let task_service = app.app.task_service.read().await;
        with_system_actor(
            "SyncTasks",
            Some(IntegrationProviderKind::Todoist),
            task_service.sync_todoist_project_contexts(app.user.id),
        )
        .await
        .unwrap();

        let contexts = list_contexts(&app).await;
        let mut names: Vec<&str> = contexts
            .iter()
            .map(|context| context.name.as_str())
            .collect();
        names.sort();
        assert_eq!(names, vec!["Archived project", "Inbox", "Project2"]);
        assert!(
            contexts
                .iter()
                .all(|context| context.status == ContextStatus::Active)
        );
    }

    #[rstest]
    #[tokio::test]
    async fn test_a_failing_context_step_does_not_change_the_task_sync(
        settings: Settings,
        #[future] authenticated_app: AuthenticatedApp,
        todoist_oauth_credential: OAuthCredentialFixture,
    ) {
        let app = authenticated_app.await;
        connect_todoist(&app, &settings, todoist_oauth_credential).await;
        // No item to sync, so no project is needed by the task sync, and the projects request
        // of the context step is not mocked: it fails
        mock_todoist_sync_resources_service(
            &app.app.todoist_mock_server,
            "items",
            &TodoistSyncResponse {
                items: Some(vec![]),
                projects: None,
                labels: None,
                full_sync: false,
                temp_id_mapping: HashMap::new(),
                sync_token: SyncToken("abcd".to_string()),
            },
            None,
        )
        .await;

        let task_creations: Vec<TaskCreationResult> = sync_tasks(
            &app.client,
            &app.app.api_address,
            Some(TaskSourceKind::Todoist),
            false,
        )
        .await;

        assert!(task_creations.is_empty());
        assert!(list_contexts(&app).await.is_empty());
        let integration_connection = get_integration_connection_per_provider(
            &app,
            app.user.id,
            IntegrationProviderKind::Todoist,
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(integration_connection.last_tasks_sync_failure_message, None);
    }
}
