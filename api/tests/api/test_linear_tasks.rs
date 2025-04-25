#![allow(clippy::too_many_arguments)]

use chrono::Utc;
use graphql_client::Response;
use pretty_assertions::assert_eq;
use rstest::*;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{
            linear::{LinearConfig, LinearSyncTaskConfig},
            todoist::TodoistConfig,
        },
    },
    notification::{NotificationSourceKind, NotificationStatus},
    task::{PresetDueDate, ProjectSummary, Task, TaskCreationResult, TaskSourceKind, TaskStatus},
    third_party::integrations::linear::{
        LinearIssue, LinearWorkflowState, LinearWorkflowStateType,
    },
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
        create_and_mock_integration_connection, nango_linear_connection, nango_todoist_connection,
    },
    notification::{
        linear::{mock_linear_update_issue_state_query, sync_linear_tasks_response},
        list_notifications_with_tasks,
    },
    rest::get_resource,
    settings,
    task::{
        linear::create_linear_task,
        sync_tasks,
        todoist::{
            mock_todoist_sync_resources_service, sync_todoist_items_response,
            sync_todoist_projects_response,
        },
    },
};

#[rstest]
#[case::with_completed_task_in_inbox("1111", TaskStatus::Done)] // Inbox
#[case::with_completed_task_not_in_inbox("2222", TaskStatus::Done)]
#[case::with_uncompleted_task_in_inbox("1111", TaskStatus::Active)] // Inbox
#[case::with_uncompleted_task_not_in_inbox("2222", TaskStatus::Active)]
#[tokio::test]
async fn test_sync_todoist_linear_task(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    mut sync_todoist_items_response: TodoistSyncResponse,
    sync_todoist_projects_response: TodoistSyncResponse,
    sync_linear_tasks_response: Response<assigned_issues_query::ResponseData>,
    nango_linear_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
    #[case] new_project_id: &str,
    #[case] expected_new_task_status: TaskStatus,
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
        name: "Inbox".to_string(),
        source_id: "1111".into(),
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
            },
        }),
        &settings,
        nango_linear_connection,
        None,
        None,
    )
    .await;

    let todoist_item = &mut sync_todoist_items_response.items.as_mut().unwrap()[1];
    // Set the status after the sync
    if expected_new_task_status == TaskStatus::Done {
        todoist_item.checked = true;
        todoist_item.completed_at = Some(Utc::now());
    } else {
        todoist_item.checked = false;
        todoist_item.completed_at = None;
    }
    todoist_item.project_id = new_project_id.to_string();
    let todoist_item_id = todoist_item.id.clone();

    let todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    );
    let todoist_tasks_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "items",
        &sync_todoist_items_response,
        None,
    );

    let linear_issues: Vec<LinearIssue> = sync_linear_tasks_response
        .data
        .clone()
        .unwrap()
        .try_into()
        .unwrap();
    let linear_issue: LinearIssue = LinearIssue {
        state: LinearWorkflowState {
            r#type: if expected_new_task_status == TaskStatus::Done {
                LinearWorkflowStateType::Unstarted
            } else {
                LinearWorkflowStateType::Completed
            },
            ..linear_issues[0].state.clone()
        },
        completed_at: (expected_new_task_status != TaskStatus::Done).then(Utc::now),
        ..linear_issues[0].clone()
    };
    let existing_task = create_linear_task(
        &app.app,
        &linear_issue,
        project,
        app.user.id,
        linear_integration_connection.id,
        todoist_integration_connection.id,
        todoist_item_id,
    )
    .await;

    let linear_update_issue_status_mock = mock_linear_update_issue_state_query(
        &app.app.linear_mock_server,
        linear_issue.id,
        linear_issue
            .get_state_id_for_task_status(expected_new_task_status)
            .unwrap(),
        true,
        None,
    );

    let task_creations: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(task_creations.len(), 2);
    for task_creation in task_creations.iter() {
        // A new notification is created only after the first sync and when the synced task is in the Inbox
        if new_project_id == "1111" {
            assert_eq!(task_creation.notifications.len(), 1);
        }
    }
    todoist_tasks_mock.assert();
    todoist_projects_mock.assert();
    linear_update_issue_status_mock.assert();

    let updated_task: Box<Task> = get_resource(
        &app.client,
        &app.app.api_address,
        "tasks",
        existing_task.id.into(),
    )
    .await;
    assert_eq!(updated_task.id, existing_task.id);
    assert_eq!(
        updated_task.source_item.source_id,
        existing_task.source_item.source_id
    );
    assert_eq!(updated_task.status, expected_new_task_status);

    if new_project_id == "1111" {
        let notifications = list_notifications_with_tasks(
            &app.client,
            &app.app.api_address,
            vec![],
            false,
            Some(existing_task.id),
            Some(NotificationSourceKind::Todoist),
            false,
        )
        .await;

        assert_eq!(notifications.len(), 1);
        assert_eq!(
            notifications[0].status,
            if expected_new_task_status == TaskStatus::Done {
                NotificationStatus::Deleted
            } else {
                NotificationStatus::Unread
            }
        );
    } else {
        let notifications = list_notifications_with_tasks(
            &app.client,
            &app.app.api_address,
            vec![],
            false,
            Some(existing_task.id),
            None,
            false,
        )
        .await;

        assert_eq!(notifications.len(), 0);
    }
}
