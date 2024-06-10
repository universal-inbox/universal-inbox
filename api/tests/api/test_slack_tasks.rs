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
    notification::{NotificationSourceKind, NotificationStatus},
    task::{Task, TaskCreationResult, TaskSourceKind, TaskStatus},
    third_party::integrations::todoist::TodoistItemPriority,
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
    job::wait_for_jobs_completion,
    notification::{
        list_notifications_with_tasks,
        slack::{
            mock_slack_fetch_channel, mock_slack_fetch_message, mock_slack_fetch_team,
            mock_slack_fetch_user, mock_slack_get_chat_permalink, mock_slack_stars_add,
            mock_slack_stars_remove, slack_push_star_added_event, slack_push_star_removed_event,
        },
    },
    rest::{create_resource_response, get_resource},
    settings,
    task::{
        list_tasks, sync_tasks,
        todoist::{
            mock_todoist_get_item_service, mock_todoist_item_add_service,
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
async fn test_sync_todoist_slack_task(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    mut sync_todoist_items_response: TodoistSyncResponse,
    sync_todoist_projects_response: TodoistSyncResponse,
    slack_push_star_added_event: Box<SlackPushEvent>,
    slack_push_star_removed_event: Box<SlackPushEvent>,
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
        None,
    )
    .await;
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

    let slack_get_chat_permalink_mock = mock_slack_get_chat_permalink(
        &app.app.slack_mock_server,
        "C05XXX",
        "1707686216.825719",
        "slack_get_chat_permalink_response.json",
    );
    let slack_fetch_user_mock = mock_slack_fetch_user(
        &app.app.slack_mock_server,
        "U05YYY", // The message's creator, not the user who starred the message
        "slack_fetch_user_response.json",
    );
    let slack_message_id = "1707686216.825719";
    let slack_fetch_message_mock = mock_slack_fetch_message(
        &app.app.slack_mock_server,
        "C05XXX",
        slack_message_id,
        "slack_fetch_message_response.json",
    );
    let slack_fetch_channel_mock = mock_slack_fetch_channel(
        &app.app.slack_mock_server,
        "C05XXX",
        "slack_fetch_channel_response.json",
    );
    let slack_fetch_team_mock = mock_slack_fetch_team(
        &app.app.slack_mock_server,
        "T05XXX",
        "slack_fetch_team_response.json",
    );

    let todoist_projects_mock = mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    );
    let todoist_item = &mut sync_todoist_items_response.items.as_mut().unwrap()[1];
    // First setting the status and project before the sync
    todoist_item.project_id = "1111".to_string();
    if expected_new_task_status == TaskStatus::Done {
        todoist_item.checked = false;
        todoist_item.completed_at = None;
    } else {
        todoist_item.checked = true;
        todoist_item.completed_at = Some(Utc::now());
    }
    let todoist_item_add_mock = mock_todoist_item_add_service(
        &app.app.todoist_mock_server,
        &todoist_item.id,
        "[🔴  *Test title* 🔴...](https://slack.com/archives/C05XXX/p1234567890)".to_string(),
        Some(
            "🔴  *Test title* 🔴\n\n- list 1\n- list 2\n1. number 1\n1. number 2\n> quote\n```$ echo Hello world```\n_Some_ `formatted` ~text~.\n\nHere is a [link](https://www.universal-inbox.com)".to_string(),
        ),
        todoist_item.project_id.clone(),
        None,
        TodoistItemPriority::P1,
    );
    let todoist_get_item_mock =
        mock_todoist_get_item_service(&app.app.todoist_mock_server, Box::new(todoist_item.clone()));

    let response = create_resource_response(
        &app.client,
        &app.app.api_address,
        "hooks/slack/events",
        if expected_new_task_status == TaskStatus::Done {
            slack_push_star_added_event.clone()
        } else {
            slack_push_star_removed_event.clone()
        },
    )
    .await;

    assert_eq!(response.status(), 200);
    assert!(wait_for_jobs_completion(&app.app.redis_storage).await);

    slack_get_chat_permalink_mock.assert();
    slack_fetch_user_mock.assert();
    slack_fetch_message_mock.assert();
    slack_fetch_channel_mock.assert();
    slack_fetch_team_mock.assert();
    todoist_projects_mock.assert();
    todoist_item_add_mock.assert();
    todoist_get_item_mock.assert();

    let tasks = list_tasks(
        &app.client,
        &app.app.api_address,
        if expected_new_task_status == TaskStatus::Done {
            TaskStatus::Active
        } else {
            TaskStatus::Done
        },
    )
    .await;

    assert_eq!(tasks.len(), 1);
    let existing_task = tasks.first().unwrap();

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
        // A new notification is created only after the first sync (ie. not when receiving
        // the Slack star added event) and when the synced task is in the Inbox
        if new_project_id == "1111" {
            assert_eq!(task_creation.notifications.len(), 1);
        }
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
        )
        .await;

        assert_eq!(notifications.len(), 0);
    }
}
