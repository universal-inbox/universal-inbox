use std::collections::HashMap;

use chrono::NaiveDate;
use pretty_assertions::assert_eq;
use rstest::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;
use uuid::Uuid;
use wiremock::matchers::{body_json, body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use universal_inbox::{
    HasHtmlUrl,
    integration_connection::integrations::todoist::SyncToken,
    notification::{NotificationSourceKind, NotificationStatus},
    task::{DueDate, TaskCreationResult, TaskStatus},
    third_party::{
        integrations::todoist::{TodoistItem, TodoistItemDue, TodoistItemPriority},
        item::ThirdPartyItemData,
    },
    user::UserId,
};

use universal_inbox_api::integrations::todoist::{
    TodoistCommandStatus, TodoistSyncCommandItemAddArgs, TodoistSyncCommandItemCompleteArgs,
    TodoistSyncCommandItemDeleteArgs, TodoistSyncCommandItemMoveArgs,
    TodoistSyncCommandItemUncompleteArgs, TodoistSyncCommandItemUpdateArgs,
    TodoistSyncCommandProjectAddArgs, TodoistSyncResponse, TodoistSyncStatusResponse,
};

use crate::helpers::load_json_fixture_file;

#[fixture]
pub fn sync_todoist_items_response() -> TodoistSyncResponse {
    load_json_fixture_file("sync_todoist_items_response.json")
}

#[fixture]
pub fn sync_todoist_projects_response() -> TodoistSyncResponse {
    load_json_fixture_file("sync_todoist_projects_response.json")
}

pub async fn mock_todoist_item_add_service(
    todoist_mock_server: &MockServer,
    new_item_id: &str,
    content: String,
    description: Option<String>,
    project_id: Option<String>,
    due: Option<TodoistItemDue>,
    priority: TodoistItemPriority,
) {
    let sync_item_add_todoist_response = TodoistSyncStatusResponse {
        sync_status: HashMap::from([(Uuid::new_v4(), TodoistCommandStatus::Ok("ok".to_string()))]),
        full_sync: false,
        temp_id_mapping: HashMap::from([(Uuid::new_v4().to_string(), new_item_id.to_string())]),
        sync_token: SyncToken("sync token".to_string()),
    };

    mock_todoist_sync_service(
        todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemAdd {
            args: TodoistSyncCommandItemAddArgs {
                content,
                description,
                project_id,
                due,
                priority,
            },
        }],
        Some(sync_item_add_todoist_response),
    )
    .await;
}

pub async fn mock_todoist_get_item_service(
    todoist_mock_server: &MockServer,
    result: Box<TodoistItem>,
) {
    let item_id = result.id.clone();

    Mock::given(method("GET"))
        .and(path(format!("/tasks/{item_id}")))
        .and(header("authorization", "Bearer todoist_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(&*result),
        )
        .mount(todoist_mock_server)
        .await;
}

pub async fn mock_todoist_delete_item_service(todoist_mock_server: &MockServer, task_id: &str) {
    mock_todoist_sync_service(
        todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemDelete {
            args: TodoistSyncCommandItemDeleteArgs {
                id: task_id.to_string(),
            },
        }],
        None,
    )
    .await;
}

pub async fn mock_todoist_complete_item_service(todoist_mock_server: &MockServer, task_id: &str) {
    mock_todoist_sync_service(
        todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemComplete {
            args: TodoistSyncCommandItemCompleteArgs {
                id: task_id.to_string(),
            },
        }],
        None,
    )
    .await;
}

pub async fn mock_todoist_uncomplete_item_service(todoist_mock_server: &MockServer, task_id: &str) {
    mock_todoist_sync_service(
        todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemUncomplete {
            args: TodoistSyncCommandItemUncompleteArgs {
                id: task_id.to_string(),
            },
        }],
        None,
    )
    .await;
}

pub async fn mock_todoist_sync_project_add(
    todoist_mock_server: &MockServer,
    new_project: &str,
    new_project_id: &str,
) {
    let sync_project_add_todoist_response = TodoistSyncStatusResponse {
        sync_status: HashMap::from([(Uuid::new_v4(), TodoistCommandStatus::Ok("ok".to_string()))]),
        full_sync: false,
        temp_id_mapping: HashMap::from([(Uuid::new_v4().to_string(), new_project_id.to_string())]),
        sync_token: SyncToken("sync token".to_string()),
    };

    mock_todoist_sync_service(
        todoist_mock_server,
        vec![TodoistSyncPartialCommand::ProjectAdd {
            args: TodoistSyncCommandProjectAddArgs {
                name: new_project.to_string(),
            },
        }],
        Some(sync_project_add_todoist_response),
    )
    .await;
}

pub async fn mock_todoist_sync_service(
    todoist_mock_server: &MockServer,
    commands: Vec<TodoistSyncPartialCommand>,
    result: Option<TodoistSyncStatusResponse>,
) {
    let body = json!({ "commands": commands });

    let response = result.unwrap_or_else(|| {
        let status: HashMap<Uuid, TodoistCommandStatus> = commands
            .iter()
            .map(|_| (Uuid::new_v4(), TodoistCommandStatus::Ok("ok".to_string())))
            .collect();
        TodoistSyncStatusResponse {
            sync_status: status,
            full_sync: false,
            temp_id_mapping: HashMap::new(),
            sync_token: SyncToken("sync token".to_string()),
        }
    });

    Mock::given(method("POST"))
        .and(path("/sync"))
        .and(body_partial_json(body))
        .and(header("authorization", "Bearer todoist_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(&response),
        )
        .mount(todoist_mock_server)
        .await;
}

pub async fn mock_todoist_sync_resources_service(
    todoist_mock_server: &MockServer,
    resource_name: &str,
    result: &TodoistSyncResponse,
    sync_token: Option<SyncToken>,
) {
    Mock::given(method("POST"))
        .and(path("/sync"))
        .and(body_json(json!({
            "sync_token": sync_token
                .map(|sync_token| sync_token.0)
                .unwrap_or_else(|| "*".to_string()),
            "resource_types": [resource_name]
        })))
        .and(header("authorization", "Bearer todoist_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(todoist_mock_server)
        .await;
}

#[fixture]
pub fn todoist_item() -> Box<TodoistItem> {
    load_json_fixture_file("todoist_item.json")
}

pub fn assert_sync_items(
    task_creations: &[TaskCreationResult],
    sync_todoist_items: &[TodoistItem],
    expected_user_id: UserId,
) {
    for task_creation in task_creations.iter() {
        let task = &task_creation.task;
        let notification = task_creation.notifications.first();

        assert_eq!(task.user_id, expected_user_id);
        match task.source_item.source_id.as_ref() {
            "1123" => {
                assert_eq!(
                    task.title,
                    "Release new version of Universal Inbox".to_string()
                );
                assert_eq!(
                    task.status,
                    if sync_todoist_items[0].checked {
                        TaskStatus::Done
                    } else {
                        TaskStatus::Active
                    }
                );
                assert_eq!(
                    task.due_at,
                    Some(DueDate::Date(NaiveDate::from_ymd_opt(2016, 9, 1).unwrap()))
                );
                assert_eq!(
                    task.get_html_url(),
                    "https://todoist.com/showTask?id=1123"
                        .parse::<Url>()
                        .unwrap()
                );
                assert_eq!(task.project, "Inbox".to_string());
                assert_eq!(
                    task.source_item.data,
                    ThirdPartyItemData::TodoistItem(Box::new(sync_todoist_items[0].clone()))
                );

                assert!(notification.is_some());
                let notif = notification.unwrap();
                assert_eq!(notif.title, task.title);
                assert_eq!(notif.kind, NotificationSourceKind::Todoist);
                assert_eq!(
                    notif.status,
                    if sync_todoist_items[0].checked {
                        NotificationStatus::Deleted
                    } else {
                        NotificationStatus::Unread
                    }
                );
                assert_eq!(notif.source_item.id, task.source_item.id);
                assert_eq!(notif.task_id, Some(task.id));
                assert_eq!(notif.user_id, expected_user_id);
            }
            // This task should be updated
            "1456" => {
                assert_eq!(task.title, "Task 2".to_string());
                assert_eq!(task.status, TaskStatus::Active);
                assert_eq!(
                    task.get_html_url(),
                    "https://todoist.com/showTask?id=1456"
                        .parse::<Url>()
                        .unwrap()
                );
                assert_eq!(task.project, "Project2".to_string());
                assert_eq!(
                    task.source_item.data,
                    ThirdPartyItemData::TodoistItem(Box::new(sync_todoist_items[1].clone()))
                );
                assert!(notification.is_some());
                let notif = notification.unwrap();
                assert_eq!(notif.status, NotificationStatus::Deleted);
                assert_eq!(notif.kind, NotificationSourceKind::Todoist);
                assert_eq!(notif.source_item.id, task.source_item.id);
            }
            _ => {
                unreachable!("Unexpected task title '{}'", &task.title);
            }
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq, Debug, Clone)]
#[serde(tag = "type")]
pub enum TodoistSyncPartialCommand {
    #[serde(rename = "item_add")]
    ItemAdd { args: TodoistSyncCommandItemAddArgs },
    #[serde(rename = "item_delete")]
    ItemDelete {
        args: TodoistSyncCommandItemDeleteArgs,
    },
    #[serde(rename = "item_complete")]
    ItemComplete {
        args: TodoistSyncCommandItemCompleteArgs,
    },
    #[serde(rename = "item_uncomplete")]
    ItemUncomplete {
        args: TodoistSyncCommandItemUncompleteArgs,
    },
    #[serde(rename = "item_update")]
    ItemUpdate {
        args: TodoistSyncCommandItemUpdateArgs,
    },
    #[serde(rename = "item_move")]
    ItemMove {
        args: TodoistSyncCommandItemMoveArgs,
    },
    #[serde(rename = "project_add")]
    ProjectAdd {
        args: TodoistSyncCommandProjectAddArgs,
    },
}
