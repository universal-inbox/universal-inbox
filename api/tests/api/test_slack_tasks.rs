#![allow(clippy::too_many_arguments)]

use chrono::{Timelike, Utc};
use pretty_assertions::assert_eq;
use rstest::*;
use slack_morphism::prelude::*;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{slack::SlackConfig, todoist::TodoistConfig},
    },
    notification::{NotificationSourceKind, NotificationStatus},
    task::{service::TaskPatch, Task, TaskCreationResult, TaskSourceKind, TaskStatus},
    third_party::{
        integrations::{
            slack::{SlackStar, SlackStarItem, SlackStarState},
            todoist::{TodoistItem, TodoistItemPriority},
        },
        item::{ThirdPartyItem, ThirdPartyItemCreationResult, ThirdPartyItemData},
    },
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
    notification::{
        list_notifications_with_tasks,
        slack::{
            mock_slack_fetch_channel, mock_slack_fetch_reply, mock_slack_fetch_team,
            mock_slack_fetch_user, mock_slack_get_chat_permalink, mock_slack_list_emojis,
            mock_slack_list_usergroups, mock_slack_stars_add, mock_slack_stars_remove,
            slack_push_star_added_event, slack_push_star_removed_event, slack_starred_message,
        },
    },
    rest::{create_resource, create_resource_response, get_resource, patch_resource},
    settings,
    task::{
        list_tasks_until, sync_tasks,
        todoist::{
            mock_todoist_complete_item_service, mock_todoist_get_item_service,
            mock_todoist_item_add_service, mock_todoist_sync_resources_service,
            sync_todoist_items_response, sync_todoist_projects_response, todoist_item,
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
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_tasks()),
        &settings,
        nango_slack_connection,
        None,
        None,
    )
    .await;
    create_and_mock_integration_connection(
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

    // Not asserting this call as it could be cached by another test
    let _slack_get_chat_permalink_mock = mock_slack_get_chat_permalink(
        &app.app.slack_mock_server,
        "C05XXX",
        "1707686216.825719",
        "slack_get_chat_permalink_response.json",
    );
    mock_slack_list_emojis(&app.app.slack_mock_server, "slack_emoji_list_response.json");
    let slack_fetch_user_mock = mock_slack_fetch_user(
        &app.app.slack_mock_server,
        "U05YYY", // The message's creator, not the user who starred the message
        "slack_fetch_user_response.json",
    );
    let slack_message_id = "1707686216.825719";
    let slack_fetch_message_mock = mock_slack_fetch_reply(
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
    let slack_list_usergroups_mock = mock_slack_list_usergroups(
        &app.app.slack_mock_server,
        "slack_list_usergroups_response.json",
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
        "[📥  Universal Inbox new release 📥...](https://slack.com/archives/C05XXX/p1234567890)"
            .to_string(),
        Some(
            r#"📥  *Universal Inbox new release* 📥
- list 1
- list 2

1. number 1
1. number 2

> quote


```
$ echo Hello world
```
\
_Some_ `formatted` ~text~.\
\
Here is a [link](https://www.universal-inbox.com)@@john.doe@@@admins@#universal-inbox
👋![:unknown2:](https://emoji.com/unknown2.png)"#
                .to_string(),
        ),
        Some(todoist_item.project_id.clone()),
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
    let tasks = list_tasks_until(
        &app.client,
        &app.app.api_address,
        if expected_new_task_status == TaskStatus::Done {
            TaskStatus::Active
        } else {
            TaskStatus::Done
        },
        1,
    )
    .await;

    slack_fetch_user_mock.assert();
    slack_fetch_message_mock.assert();
    slack_fetch_channel_mock.assert();
    slack_fetch_team_mock.assert();
    slack_list_usergroups_mock.assert();
    todoist_projects_mock.assert();
    todoist_item_add_mock.assert();
    todoist_get_item_mock.assert();

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
    if let Some(slack_stars_remove_mock) = slack_stars_remove_mock {
        slack_stars_remove_mock.assert();
    } else {
        slack_stars_add_mock.assert();
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

#[rstest]
#[tokio::test]
async fn test_patch_slack_task_status_as_done(
    settings: Settings,
    #[future] authenticated_app: AuthenticatedApp,
    slack_push_star_added_event: Box<SlackPushEvent>,
    slack_starred_message: Box<SlackStarItem>,
    todoist_item: Box<TodoistItem>,
    sync_todoist_projects_response: TodoistSyncResponse,
    nango_slack_connection: Box<NangoConnection>,
    nango_todoist_connection: Box<NangoConnection>,
) {
    let app = authenticated_app.await;
    let integration_connection = create_and_mock_integration_connection(
        &app.app,
        app.user.id,
        &settings.oauth2.nango_secret_key,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_tasks()),
        &settings,
        nango_slack_connection,
        None,
        None,
    )
    .await;
    create_and_mock_integration_connection(
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

    mock_todoist_sync_resources_service(
        &app.app.todoist_mock_server,
        "projects",
        &sync_todoist_projects_response,
        None,
    );

    mock_todoist_item_add_service(
        &app.app.todoist_mock_server,
        &todoist_item.id,
        "[📥  Universal Inbox new release 📥...](https://example.com/)".to_string(),
        Some(
            r#"📥  *Universal Inbox new release* 📥
- list 1
- list 2

1. number 1
1. number 2

> quote


```
$ echo Hello world
```
\
_Some_ `formatted` ~text~.\
\
Here is a [link](https://www.universal-inbox.com)@@john.doe@@@admins@#universal-inbox
👋![:unknown2:](https://emoji.com/unknown2.png)"#
                .to_string(),
        ),
        Some(todoist_item.project_id.clone()),
        None,
        TodoistItemPriority::P1,
    );
    mock_todoist_get_item_service(
        &app.app.todoist_mock_server,
        Box::new(*todoist_item.clone()),
    );

    let SlackPushEvent::EventCallback(SlackPushEventCallback {
        event:
            SlackEventCallbackBody::StarAdded(
                SlackStarAddedEvent {
                    item:
                        SlackStarsItem::Message(SlackStarsItemMessage {
                            message:
                                SlackHistoryMessage {
                                    origin: SlackMessageOrigin { ts: source_id, .. },
                                    ..
                                },
                            ..
                        }),
                    ..
                },
                ..,
            ),
        ..
    }) = *slack_push_star_added_event
    else {
        unreachable!("Unexpected event type");
    };
    let creation: Box<ThirdPartyItemCreationResult> = create_resource(
        &app.client,
        &app.app.api_address,
        "third_party/task/items",
        Box::new(ThirdPartyItem {
            id: Uuid::new_v4().into(),
            source_id: source_id.to_string(),
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            updated_at: Utc::now().with_nanosecond(0).unwrap(),
            user_id: app.user.id,
            data: ThirdPartyItemData::SlackStar(Box::new(SlackStar {
                state: SlackStarState::StarAdded,
                created_at: Utc::now().with_nanosecond(0).unwrap(),
                item: *slack_starred_message,
            })),
            integration_connection_id: integration_connection.id,
            source_item: None,
        }),
    )
    .await;

    let exiting_task = creation.task.as_ref().unwrap().clone();
    assert_eq!(exiting_task.status, TaskStatus::Active);

    let todoist_mock = mock_todoist_complete_item_service(
        &app.app.todoist_mock_server,
        &exiting_task.sink_item.as_ref().unwrap().source_id,
    );
    let slack_star_remove_mock =
        mock_slack_stars_remove(&app.app.slack_mock_server, "C05XXX", "1707686216.825719");

    let patched_task: Box<Task> = patch_resource(
        &app.client,
        &app.app.api_address,
        "tasks",
        exiting_task.id.into(),
        &TaskPatch {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .await;

    todoist_mock.assert();
    slack_star_remove_mock.assert();

    assert!(patched_task.completed_at.is_some());
    assert_eq!(
        patched_task,
        Box::new(Task {
            status: TaskStatus::Done,
            completed_at: patched_task.completed_at,
            ..exiting_task
        })
    );
}
