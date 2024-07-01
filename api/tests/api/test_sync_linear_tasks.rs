use graphql_client::Response;
use rstest::*;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{
            linear::{LinearConfig, LinearSyncTaskConfig},
            todoist::TodoistConfig,
        },
        provider::IntegrationProviderKind,
        IntegrationConnectionStatus,
    },
    task::{ProjectSummary, TaskCreationResult, TaskSourceKind, TaskStatus},
    third_party::{
        integrations::{
            linear::LinearIssue,
            todoist::{TodoistItem, TodoistItemPriority},
        },
        item::ThirdPartyItemKind,
    },
    HasHtmlUrl,
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{
        linear::graphql::assigned_issues_query, oauth2::NangoConnection,
        todoist::TodoistSyncResponse,
    },
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_and_mock_integration_connection, get_integration_connection_per_provider,
        nango_linear_connection, nango_todoist_connection,
    },
    notification::linear::{mock_linear_assigned_issues_query, sync_linear_tasks_response},
    settings,
    task::{
        linear::create_linear_task,
        sync_tasks,
        todoist::{
            mock_todoist_complete_item_service, mock_todoist_get_item_service,
            mock_todoist_item_add_service, mock_todoist_sync_resources_service,
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
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
        None,
    )
    .await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Linear(LinearConfig {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig {
                enabled: true,
                target_project: Some(ProjectSummary {
                    name: "Project2".to_string(),
                    source_id: "2222".to_string(),
                }),
            },
        }),
        &settings,
        nango_linear_connection,
        None,
    )
    .await;

    let sync_linear_issues: Vec<LinearIssue> = sync_linear_tasks_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();
    let linear_assigned_issues_mock =
        mock_linear_assigned_issues_query(&app.app.linear_mock_server, &sync_linear_tasks_response);

    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    );
    let expected_task_title = format!(
        "[{}]({})",
        sync_linear_issues[0].title.clone(),
        sync_linear_issues[0].get_html_url()
    );
    let todoist_item_add_mock = mock_todoist_item_add_service(
        &app.app.todoist_mock_server,
        &todoist_item.id,
        expected_task_title.clone(),
        sync_linear_issues[0].description.clone(),
        "2222".to_string(), // ie. "Project2"
        None,
        TodoistItemPriority::P1,
    );
    let todoist_get_item_mock =
        mock_todoist_get_item_service(&app.app.todoist_mock_server, todoist_item.clone());

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
    let sink_item = task.sink_item.clone().unwrap();
    assert_eq!(sink_item.kind(), ThirdPartyItemKind::TodoistItem);
    assert_eq!(sink_item.source_id, todoist_item.id);

    linear_assigned_issues_mock.assert();
    todoist_item_add_mock.assert();
    todoist_get_item_mock.assert();

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
    assert!(integration_connection
        .last_tasks_sync_completed_at
        .is_some());
    assert!(integration_connection
        .last_tasks_sync_failure_message
        .is_none());
    assert_eq!(integration_connection.tasks_sync_failures, 0);
    assert_eq!(
        integration_connection.status,
        IntegrationConnectionStatus::Validated
    );
    assert!(integration_connection.failure_message.is_none(),);
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
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
        None,
    )
    .await;
    let project = ProjectSummary {
        name: "Project2".to_string(),
        source_id: "2222".to_string(),
    };
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Linear(LinearConfig {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig {
                enabled: true,
                target_project: Some(project.clone()),
            },
        }),
        &settings,
        nango_linear_connection,
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
    let linear_assigned_issues_mock = mock_linear_assigned_issues_query(
        &app.app.linear_mock_server,
        &empty_sync_linear_tasks_response,
    );

    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    );
    let todoist_complete_item_mock = mock_todoist_complete_item_service(
        &app.app.todoist_mock_server,
        &existing_task.sink_item.as_ref().unwrap().source_id,
    );

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

    linear_assigned_issues_mock.assert();
    todoist_complete_item_mock.assert();
}
