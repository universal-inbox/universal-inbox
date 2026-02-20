use std::collections::HashMap;

use graphql_client::Response;
use pretty_assertions::assert_ne;
use rstest::*;
use tokio::time::{Duration, sleep};
use uuid::Uuid;

use universal_inbox::{
    HasHtmlUrl,
    integration_connection::{
        IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::{
            linear::{LinearConfig, LinearSyncTaskConfig},
            todoist::{SyncToken, TodoistConfig},
        },
        provider::IntegrationProviderKind,
    },
    notification::NotificationStatus,
    task::{
        DueDate, PresetDueDate, ProjectSummary, TaskCreationConfig, TaskCreationResult,
        TaskPriority, TaskSourceKind, TaskStatus,
    },
    third_party::{
        integrations::{
            linear::{LinearIssue, LinearNotification},
            todoist::{TodoistItem, TodoistItemPriority},
        },
        item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemKind},
    },
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{
        linear::graphql::{assigned_issues_query, notifications_query},
        oauth2::NangoConnection,
        task::ThirdPartyTaskService,
        todoist::{
            TodoistCommandStatus, TodoistSyncCommandItemCompleteArgs, TodoistSyncResponse,
            TodoistSyncStatusResponse,
        },
    },
    repository::{task::TaskRepository, third_party::ThirdPartyItemRepository},
};

use crate::helpers::{
    auth::{AuthenticatedApp, authenticated_app},
    integration_connection::{
        create_and_mock_integration_connection, get_integration_connection_per_provider,
        nango_linear_connection, nango_todoist_connection,
    },
    notification::{
        linear::{
            create_notification_from_linear_notification, mock_linear_assigned_issues_query,
            sync_linear_notifications_response, sync_linear_tasks_response,
        },
        list_notifications,
    },
    settings,
    task::{
        get_task,
        linear::create_linear_task,
        sync_tasks,
        todoist::{
            TodoistSyncPartialCommand, mock_todoist_complete_item_service,
            mock_todoist_get_item_service, mock_todoist_item_add_service,
            mock_todoist_sync_resources_service, mock_todoist_sync_service,
            sync_todoist_projects_response, todoist_item,
        },
    },
};

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_create_new_task(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    sync_linear_tasks_response: Response<assigned_issues_query::ResponseData>,
    todoist_item: Box<TodoistItem>,
    sync_todoist_projects_response: TodoistSyncResponse,
    nango_linear_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let _todoist_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
        None,
        None,
    )
    .await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Linear(LinearConfig {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig {
                enabled: true,
                target_project: Some(ProjectSummary {
                    name: "Project2".to_string(),
                    source_id: "2222".into(),
                }),
                default_due_at: Some(PresetDueDate::Today),
                ..Default::default()
            },
        }),
        &settings,
        nango_linear_connection,
        None,
        None,
    )
    .await;

    let sync_linear_issues: Vec<LinearIssue> = sync_linear_tasks_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();
    let _linear_assigned_issues_mock =
        mock_linear_assigned_issues_query(&app.app.linear_mock_server, &sync_linear_tasks_response)
            .await;

    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;
    let expected_task_title = format!(
        "[{}]({})",
        sync_linear_issues[0].title.clone(),
        sync_linear_issues[0].get_html_url()
    );
    let due_at: DueDate = PresetDueDate::Today.into();
    let _todoist_item_add_mock = mock_todoist_item_add_service(
        &app.app.todoist_mock_server,
        &todoist_item.id,
        expected_task_title.clone(),
        sync_linear_issues[0].description.clone(),
        Some("2222".to_string()), // ie. "Project2"
        Some((&due_at).into()),
        TodoistItemPriority::P1,
    )
    .await;
    let _todoist_get_item_mock =
        mock_todoist_get_item_service(&app.app.todoist_mock_server, todoist_item.clone()).await;

    let task_creation_results: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Linear),
        false,
    )
    .await;

    assert_eq!(task_creation_results.len(), 1);
    assert!(task_creation_results[0].notifications.is_empty());
    let task = &task_creation_results[0].task;
    assert_eq!(task.title, expected_task_title);
    assert_eq!(
        task.body,
        sync_linear_issues[0]
            .description
            .clone()
            .unwrap_or_default()
    );
    assert_eq!(task.status, TaskStatus::Active);
    assert_eq!(task.kind, TaskSourceKind::Linear);
    assert_eq!(task.source_item.kind(), ThirdPartyItemKind::LinearIssue);
    assert_eq!(
        task.source_item.source_id,
        sync_linear_issues[0].id.to_string()
    );
    assert_eq!(task.project, "Project2".to_string());
    assert_eq!(task.due_at, Some(due_at));

    let sink_item = task.sink_item.clone().unwrap();
    assert_eq!(sink_item.kind(), ThirdPartyItemKind::TodoistItem);
    assert_eq!(sink_item.source_id, todoist_item.id);

    let integration_connection = get_integration_connection_per_provider(
        &app,
        app.user.id,
        IntegrationProviderKind::Linear,
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

/// Test when syncing an already existing task, the existing values `due_at` and `project` are
/// are not overwritten by the default values from the Linear configuration.
#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_not_update_default_values(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    sync_linear_tasks_response: Response<assigned_issues_query::ResponseData>,
    nango_linear_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let todoist_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
        None,
        None,
    )
    .await;
    let project = ProjectSummary {
        name: "Project2".to_string(),
        source_id: "2222".into(),
    };
    let linear_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Linear(LinearConfig {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig {
                enabled: true,
                target_project: Some(project.clone()),
                // Test will create a task with due date set to Today
                default_due_at: Some(PresetDueDate::Tomorrow),
                ..Default::default()
            },
        }),
        &settings,
        nango_linear_connection,
        None,
        None,
    )
    .await;

    let linear_issues: Vec<LinearIssue> = sync_linear_tasks_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();
    let linear_issue: &LinearIssue = &linear_issues[0];
    let existing_task = create_linear_task(
        &app.app,
        linear_issue,
        ProjectSummary {
            name: "Project1".to_string(),
            source_id: "1111".into(),
        },
        app.user.id,
        linear_integration_connection.id,
        todoist_integration_connection.id,
        "todoist_source_id".to_string(),
    )
    .await;
    assert_eq!(existing_task.due_at, Some(PresetDueDate::Today.into()));

    // Sleep so that third party item created during sync will have a different `updated_at`
    // and thus force to update the task
    sleep(Duration::from_secs(1)).await;

    let source_linear_issue = sync_linear_tasks_response
        .data
        .clone()
        .unwrap()
        .issues
        .nodes[0]
        .clone();
    let single_sync_linear_tasks_response = Response {
        data: Some(assigned_issues_query::ResponseData {
            issues: assigned_issues_query::AssignedIssuesQueryIssues {
                nodes: vec![source_linear_issue],
            },
        }),
        errors: None,
        extensions: None,
    };
    let _linear_assigned_issues_mock = mock_linear_assigned_issues_query(
        &app.app.linear_mock_server,
        &single_sync_linear_tasks_response,
    )
    .await;

    let task_creation_results: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Linear),
        false,
    )
    .await;
    assert_eq!(task_creation_results.len(), 1);
    assert_eq!(task_creation_results[0].task.id, existing_task.id);

    let task = get_task(&app.client, &app.app.api_address, existing_task.id)
        .await
        .unwrap();
    assert_eq!(task.id, existing_task.id);
    assert_eq!(task.due_at, existing_task.due_at);
    assert_eq!(task.project, existing_task.project);
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_complete_existing_task(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    sync_linear_tasks_response: Response<assigned_issues_query::ResponseData>,
    sync_todoist_projects_response: TodoistSyncResponse,
    nango_linear_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let todoist_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
        None,
        None,
    )
    .await;
    let project = ProjectSummary {
        name: "Project2".to_string(),
        source_id: "2222".into(),
    };
    let linear_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Linear(LinearConfig {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig {
                enabled: true,
                target_project: Some(project.clone()),
                default_due_at: None,
                ..Default::default()
            },
        }),
        &settings,
        nango_linear_connection,
        None,
        None,
    )
    .await;

    let linear_issues: Vec<LinearIssue> = sync_linear_tasks_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();
    let linear_issue: &LinearIssue = &linear_issues[0];
    let existing_task = create_linear_task(
        &app.app,
        linear_issue,
        project,
        app.user.id,
        linear_integration_connection.id,
        todoist_integration_connection.id,
        "todoist_source_id".to_string(),
    )
    .await;
    let empty_sync_linear_tasks_response = Response {
        data: Some(assigned_issues_query::ResponseData {
            issues: assigned_issues_query::AssignedIssuesQueryIssues { nodes: vec![] },
        }),
        errors: None,
        extensions: None,
    };
    let _linear_assigned_issues_mock = mock_linear_assigned_issues_query(
        &app.app.linear_mock_server,
        &empty_sync_linear_tasks_response,
    )
    .await;

    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;
    let _todoist_complete_item_mock = mock_todoist_complete_item_service(
        &app.app.todoist_mock_server,
        &existing_task.sink_item.as_ref().unwrap().source_id,
    )
    .await;

    let task_creation_results: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Linear),
        false,
    )
    .await;

    assert_eq!(task_creation_results.len(), 1);
    assert!(task_creation_results[0].notifications.is_empty());
    let task = &task_creation_results[0].task;
    assert_eq!(task.id, existing_task.id);
    assert_eq!(task.status, TaskStatus::Done);
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_complete_existing_task_and_recreate_sink_task_if_upstream_task_does_not_exist_anymore(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    sync_linear_tasks_response: Response<assigned_issues_query::ResponseData>,
    sync_todoist_projects_response: TodoistSyncResponse,
    nango_linear_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let todoist_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
        None,
        None,
    )
    .await;
    let project = ProjectSummary {
        name: "Project2".to_string(),
        source_id: "2222".into(),
    };
    let linear_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Linear(LinearConfig {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig {
                enabled: true,
                target_project: Some(project.clone()),
                default_due_at: None,
                ..Default::default()
            },
        }),
        &settings,
        nango_linear_connection,
        None,
        None,
    )
    .await;

    let linear_issues: Vec<LinearIssue> = sync_linear_tasks_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();
    let linear_issue: &LinearIssue = &linear_issues[0];
    let existing_task = create_linear_task(
        &app.app,
        linear_issue,
        project,
        app.user.id,
        linear_integration_connection.id,
        todoist_integration_connection.id,
        "todoist_source_id".to_string(),
    )
    .await;
    let existing_sink_item = existing_task.sink_item.as_ref().unwrap().clone();
    let ThirdPartyItemData::TodoistItem(todoist_item) = existing_sink_item.data else {
        panic!("Expected sink item to be a Todoist item");
    };
    let empty_sync_linear_tasks_response = Response {
        data: Some(assigned_issues_query::ResponseData {
            issues: assigned_issues_query::AssignedIssuesQueryIssues { nodes: vec![] },
        }),
        errors: None,
        extensions: None,
    };
    let _linear_assigned_issues_mock = mock_linear_assigned_issues_query(
        &app.app.linear_mock_server,
        &empty_sync_linear_tasks_response,
    )
    .await;

    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;

    let _todoist_complete_item_mock = mock_todoist_sync_service(
        &app.app.todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemComplete {
            args: TodoistSyncCommandItemCompleteArgs {
                id: existing_task.sink_item.as_ref().unwrap().source_id.clone(),
            },
        }],
        Some(TodoistSyncStatusResponse {
            sync_status: HashMap::from([(
                Uuid::new_v4(),
                TodoistCommandStatus::Error {
                    error_code: 22,
                    error: "Item not found".to_string(),
                },
            )]),
            full_sync: false,
            temp_id_mapping: HashMap::new(),
            sync_token: SyncToken("sync token".to_string()),
        }),
    )
    .await;
    let new_todoist_item_id = "another_id".to_string();
    let _todoist_item_add_mock = mock_todoist_item_add_service(
        &app.app.todoist_mock_server,
        &new_todoist_item_id,
        existing_task.title.clone(),
        Some(existing_task.body.clone()),
        Some("2222".to_string()), // ie. "Project2"
        Some((&Into::<DueDate>::into(PresetDueDate::Today)).into()),
        TodoistItemPriority::P1,
    )
    .await;
    let _todoist_get_item_mock = mock_todoist_get_item_service(
        &app.app.todoist_mock_server,
        Box::new(TodoistItem {
            id: new_todoist_item_id.clone(),
            ..*todoist_item.clone()
        }),
    )
    .await;

    let task_creation_results: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Linear),
        false,
    )
    .await;

    assert_eq!(task_creation_results.len(), 1);
    assert!(task_creation_results[0].notifications.is_empty());
    let task = &task_creation_results[0].task;
    assert_eq!(task.id, existing_task.id);
    assert_eq!(task.status, TaskStatus::Done);
    assert_ne!(task.sink_item.as_ref().unwrap().id, existing_sink_item.id);
    assert_eq!(
        task.sink_item.as_ref().unwrap().source_id,
        new_todoist_item_id
    );
}

#[rstest]
#[tokio::test]
async fn test_sync_tasks_should_create_sink_item_if_missing_when_updating_task(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    sync_linear_tasks_response: Response<assigned_issues_query::ResponseData>,
    sync_todoist_projects_response: TodoistSyncResponse,
    todoist_item: Box<TodoistItem>,
    nango_linear_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let _todoist_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
        None,
        None,
    )
    .await;
    let project = ProjectSummary {
        name: "Project2".to_string(),
        source_id: "2222".into(),
    };
    let linear_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Linear(LinearConfig {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig {
                enabled: true,
                target_project: Some(project.clone()),
                default_due_at: None,
                ..Default::default()
            },
        }),
        &settings,
        nango_linear_connection,
        None,
        None,
    )
    .await;

    let linear_issues: Vec<LinearIssue> = sync_linear_tasks_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();
    let linear_issue: &LinearIssue = &linear_issues[0];

    let mut transaction = app.app.repository.begin().await.unwrap();
    let source_third_party_item = ThirdPartyItem::new(
        linear_issue.id.to_string(),
        ThirdPartyItemData::LinearIssue(Box::new(linear_issue.clone())),
        app.user.id,
        linear_integration_connection.id,
    );
    let source_third_party_item = app
        .app
        .repository
        .create_or_update_third_party_item(&mut transaction, Box::new(source_third_party_item))
        .await
        .unwrap()
        .value();

    let task_request = app
        .app
        .task_service
        .read()
        .await
        .linear_service
        .third_party_item_into_task(
            &mut transaction,
            linear_issue,
            &source_third_party_item,
            Some(TaskCreationConfig {
                project_name: Some(project.name.clone()),
                due_at: Some(PresetDueDate::Today.into()),
                priority: TaskPriority::default(),
            }),
            app.user.id,
        )
        .await
        .unwrap();

    let upsert_task = app
        .app
        .repository
        .create_or_update_task(&mut transaction, task_request)
        .await
        .unwrap();

    let existing_task = upsert_task.value();
    assert!(
        existing_task.sink_item.is_none(),
        "Task should have no sink_item"
    );
    transaction.commit().await.unwrap();

    sleep(Duration::from_secs(1)).await;

    let source_linear_issue = sync_linear_tasks_response
        .data
        .clone()
        .unwrap()
        .issues
        .nodes[0]
        .clone();
    let single_sync_linear_tasks_response = Response {
        data: Some(assigned_issues_query::ResponseData {
            issues: assigned_issues_query::AssignedIssuesQueryIssues {
                nodes: vec![source_linear_issue],
            },
        }),
        errors: None,
        extensions: None,
    };
    let _linear_assigned_issues_mock = mock_linear_assigned_issues_query(
        &app.app.linear_mock_server,
        &single_sync_linear_tasks_response,
    )
    .await;

    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;

    let new_todoist_item_id = "new_todoist_id".to_string();
    let _todoist_item_add_mock = mock_todoist_item_add_service(
        &app.app.todoist_mock_server,
        &new_todoist_item_id,
        existing_task.title.clone(),
        Some(existing_task.body.clone()),
        Some("2222".to_string()),
        Some((&Into::<DueDate>::into(PresetDueDate::Today)).into()),
        TodoistItemPriority::P1,
    )
    .await;
    let _todoist_get_item_mock = mock_todoist_get_item_service(
        &app.app.todoist_mock_server,
        Box::new(TodoistItem {
            id: new_todoist_item_id.clone(),
            ..*todoist_item.clone()
        }),
    )
    .await;

    let task_creation_results: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Linear),
        false,
    )
    .await;

    assert_eq!(task_creation_results.len(), 1);
    let task = &task_creation_results[0].task;
    assert_eq!(task.id, existing_task.id);
    assert!(task.sink_item.is_some(), "Task should now have a sink_item");
    assert_eq!(
        task.sink_item.as_ref().unwrap().source_id,
        new_todoist_item_id
    );
}

/// Test Path 1: When syncing a Linear issue as a task, existing Linear notifications
/// for the same issue should be automatically deleted.
#[rstest]
#[tokio::test]
#[allow(clippy::too_many_arguments)]
async fn test_sync_tasks_should_auto_delete_existing_linear_notification(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    sync_linear_tasks_response: Response<assigned_issues_query::ResponseData>,
    sync_linear_notifications_response: Response<notifications_query::ResponseData>,
    todoist_item: Box<TodoistItem>,
    sync_todoist_projects_response: TodoistSyncResponse,
    nango_linear_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let _todoist_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
        None,
        None,
    )
    .await;
    let linear_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Linear(LinearConfig {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig {
                enabled: true,
                target_project: Some(ProjectSummary {
                    name: "Project2".to_string(),
                    source_id: "2222".into(),
                }),
                default_due_at: Some(PresetDueDate::Today),
                ..Default::default()
            },
        }),
        &settings,
        nango_linear_connection,
        None,
        None,
    )
    .await;

    // Create a notification from the Linear notification fixture that references the same
    // issue as the task fixture (index [2] has issue_id matching the task fixture's issue).
    let sync_linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();
    let issue_notification = &sync_linear_notifications[2];
    let existing_notification = create_notification_from_linear_notification(
        &app.app,
        issue_notification,
        app.user.id,
        linear_integration_connection.id,
    )
    .await;
    assert_eq!(existing_notification.status, NotificationStatus::Read);

    // Set up mocks for task sync
    let sync_linear_issues: Vec<LinearIssue> = sync_linear_tasks_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();
    let _linear_assigned_issues_mock =
        mock_linear_assigned_issues_query(&app.app.linear_mock_server, &sync_linear_tasks_response)
            .await;

    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    )
    .await;
    let expected_task_title = format!(
        "[{}]({})",
        sync_linear_issues[0].title.clone(),
        sync_linear_issues[0].get_html_url()
    );
    let due_at: DueDate = PresetDueDate::Today.into();
    let _todoist_item_add_mock = mock_todoist_item_add_service(
        &app.app.todoist_mock_server,
        &todoist_item.id,
        expected_task_title.clone(),
        sync_linear_issues[0].description.clone(),
        Some("2222".to_string()),
        Some((&due_at).into()),
        TodoistItemPriority::P1,
    )
    .await;
    let _todoist_get_item_mock =
        mock_todoist_get_item_service(&app.app.todoist_mock_server, todoist_item.clone()).await;

    // Trigger task sync — this should auto-delete the notification
    let task_creation_results: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Linear),
        false,
    )
    .await;

    assert_eq!(task_creation_results.len(), 1);

    // Verify the notification is now Deleted
    let notifications = list_notifications(
        &app.client,
        &app.app.api_address,
        vec![
            NotificationStatus::Read,
            NotificationStatus::Unread,
            NotificationStatus::Deleted,
        ],
        false,
        None,
        None,
        false,
    )
    .await;

    let auto_deleted_notification = notifications
        .iter()
        .find(|n| n.id == existing_notification.id)
        .expect("Notification should still exist in the database");
    assert_eq!(
        auto_deleted_notification.status,
        NotificationStatus::Deleted,
        "Notification for the synced Linear issue should be auto-deleted"
    );
}

/// Test Path 2: When creating a Linear notification for an issue that already has
/// a synced task, the notification should be immediately marked as Deleted.
#[rstest]
#[tokio::test]
async fn test_create_notification_should_auto_delete_when_task_already_exists(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    sync_linear_tasks_response: Response<assigned_issues_query::ResponseData>,
    sync_linear_notifications_response: Response<notifications_query::ResponseData>,
    nango_linear_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let todoist_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
        None,
        None,
    )
    .await;
    let project = ProjectSummary {
        name: "Project2".to_string(),
        source_id: "2222".into(),
    };
    let linear_integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Linear(LinearConfig {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig {
                enabled: true,
                target_project: Some(project.clone()),
                default_due_at: Some(PresetDueDate::Today),
                ..Default::default()
            },
        }),
        &settings,
        nango_linear_connection,
        None,
        None,
    )
    .await;

    // First, create a task from the Linear issue
    let linear_issues: Vec<LinearIssue> = sync_linear_tasks_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();
    let linear_issue: &LinearIssue = &linear_issues[0];
    let _existing_task = create_linear_task(
        &app.app,
        linear_issue,
        project,
        app.user.id,
        linear_integration_connection.id,
        todoist_integration_connection.id,
        "todoist_source_id".to_string(),
    )
    .await;

    // Now create a notification for the same issue — it should be auto-deleted
    let sync_linear_notifications: Vec<LinearNotification> = sync_linear_notifications_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();
    let issue_notification = &sync_linear_notifications[2];
    let notification = create_notification_from_linear_notification(
        &app.app,
        issue_notification,
        app.user.id,
        linear_integration_connection.id,
    )
    .await;

    // The returned notification should already be marked as Deleted
    assert_eq!(
        notification.status,
        NotificationStatus::Deleted,
        "Notification for a Linear issue that already has a synced task should be auto-deleted"
    );
}
