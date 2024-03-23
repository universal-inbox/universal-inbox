#![allow(clippy::too_many_arguments)]

use chrono::Utc;
use pretty_assertions::assert_eq;
use rstest::*;
use slack_morphism::prelude::SlackPushEvent;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{slack::SlackConfig, todoist::TodoistConfig},
    },
    notification::{
        integrations::slack::SlackPushEventCallbackExt, Notification, NotificationStatus,
    },
    task::{Task, TaskCreationResult, TaskSourceKind, TaskStatus},
};

use universal_inbox_api::{
    configuration::Settings,
    integrations::{oauth2::NangoConnection, todoist::TodoistSyncResponse},
};

use crate::helpers::{
    auth::{authenticated_app, AuthenticatedApp},
    integration_connection::{
        create_and_mock_integration_connection, nango_slack_connection, nango_todoist_connection,
    },
    notification::slack::{
        mock_slack_stars_add, mock_slack_stars_remove, slack_push_star_added_event,
    },
    rest::{create_resource, get_resource},
    settings,
    task::{
        sync_tasks,
        todoist::{
            create_task_from_todoist_item, mock_todoist_sync_resources_service,
            sync_todoist_items_response, sync_todoist_projects_response,
        },
    },
};

#[rstest]
#[case::with_completed_task_in_inbox("1111", TaskStatus::Done)] // Inbox
#[case::with_completed_task_not_in_inbox("2222", TaskStatus::Done)]
#[case::with_uncompleted_task_in_inbox("1111", TaskStatus::Active)] // Inbox
#[case::with_uncompleted_task_not_in_inbox("2222", TaskStatus::Active)]
#[tokio::test]
async fn test_sync_todoist_slack_task_as_complete(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    mut sync_todoist_items_response: TodoistSyncResponse,
    sync_todoist_projects_response: TodoistSyncResponse,
    slack_push_star_added_event: Box<SlackPushEvent>,
    nango_slack_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
    #[case] new_project_id: &str,
    #[case] expected_new_task_status: TaskStatus,
) {
    let app = authenticated_app.await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_tasks()),
        &settings,
        nango_slack_connection,
    )
    .await;
    create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.integrations.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Todoist(TodoistConfig::enabled()),
        &settings,
        nango_todoist_connection,
    )
    .await;

    let todoist_item = &mut sync_todoist_items_response.items.as_mut().unwrap()[1];
    // First setting the status before the sync
    if expected_new_task_status == TaskStatus::Done {
        todoist_item.checked = false;
        todoist_item.completed_at = None;
    } else {
        todoist_item.checked = true;
        todoist_item.completed_at = Some(Utc::now());
    }
    let existing_task_creation = create_task_from_todoist_item(
        &app.client,
        &app.app.api_address,
        &*todoist_item,
        "Project 2".to_string(),
        app.user.id,
    )
    .await;
    let existing_task = existing_task_creation.task;

    let SlackPushEvent::EventCallback(star_added_event) = *slack_push_star_added_event else {
        unreachable!("Unexpected event type");
    };

    let existing_notification: Box<Notification> = create_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        Box::new(Notification {
            status: NotificationStatus::Deleted,
            task_id: Some(existing_task.id),
            ..star_added_event.into_notification(app.user.id).unwrap()
        }),
    )
    .await;

    // Set the status after the sync
    if expected_new_task_status == TaskStatus::Done {
        todoist_item.checked = true;
        todoist_item.completed_at = Some(Utc::now());
    } else {
        todoist_item.checked = false;
        todoist_item.completed_at = None;
    }
    todoist_item.project_id = new_project_id.to_string();
    let todoist_tasks_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "items",
        &sync_todoist_items_response,
        None,
    );
    let todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    );

    let slack_stars_add_mock =
        mock_slack_stars_add(&app.app.slack_mock_server, "C05XXX", "1707686216.825719");
    let slack_stars_remove_mock = (expected_new_task_status == TaskStatus::Done).then(|| {
        mock_slack_stars_remove(&app.app.slack_mock_server, "C05XXX", "1707686216.825719")
    });

    let task_creations: Vec<TaskCreationResult> = sync_tasks(
        &app.client,
        &app.app.api_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;

    assert_eq!(task_creations.len(), 2);
    for task_creation in task_creations.iter() {
        assert_eq!(task_creation.notifications.len(), 1);
    }
    todoist_tasks_mock.assert();
    todoist_projects_mock.assert();
    slack_stars_add_mock.assert();
    if let Some(slack_stars_remove_mock) = slack_stars_remove_mock {
        slack_stars_remove_mock.assert();
    }

    let updated_task: Box<Task> = get_resource(
        &app.client,
        &app.app.api_address,
        "tasks",
        existing_task.id.into(),
    )
    .await;
    assert_eq!(updated_task.id, existing_task.id);
    assert_eq!(updated_task.source_id, existing_task.source_id);
    assert_eq!(updated_task.status, expected_new_task_status);

    let updated_notification: Box<Notification> = get_resource(
        &app.client,
        &app.app.api_address,
        "notifications",
        existing_notification.id.into(),
    )
    .await;
    assert_eq!(updated_notification.id, existing_notification.id);
    assert_eq!(
        updated_notification.source_id,
        existing_notification.source_id
    );
    assert_eq!(updated_notification.status, NotificationStatus::Deleted);
}
