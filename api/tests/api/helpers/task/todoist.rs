use std::collections::HashMap;

use chrono::{NaiveDate, TimeZone, Utc};
use http::Uri;
use httpmock::{Method::POST, Mock, MockServer};
use pretty_assertions::assert_eq;
use reqwest::Client;
use rstest::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::debug;
use uuid::Uuid;

use universal_inbox::{
    notification::{NotificationMetadata, NotificationStatus},
    task::{
        integrations::todoist::{self, TodoistItem, TodoistItemDue, TodoistItemPriority},
        DueDate, Task, TaskMetadata, TaskStatus,
    },
};

use universal_inbox_api::{
    integrations::todoist::{
        TodoistCommandStatus, TodoistItemInfoResponse, TodoistSyncCommandItemAddArgs,
        TodoistSyncCommandItemCompleteArgs, TodoistSyncCommandItemDeleteArgs,
        TodoistSyncCommandItemMoveArgs, TodoistSyncCommandItemUpdateArgs,
        TodoistSyncCommandProjectAddArgs, TodoistSyncResponse, TodoistSyncStatusResponse,
    },
    universal_inbox::task::TaskCreationResult,
};

use crate::helpers::{load_json_fixture_file, rest::create_resource};

#[fixture]
pub fn sync_todoist_items_response() -> TodoistSyncResponse {
    load_json_fixture_file("/tests/api/fixtures/sync_todoist_items_response.json")
}

#[fixture]
pub fn sync_todoist_projects_response() -> TodoistSyncResponse {
    load_json_fixture_file("/tests/api/fixtures/sync_todoist_projects_response.json")
}

pub fn mock_todoist_item_add_service<'a>(
    todoist_mock_server: &'a MockServer,
    new_item_id: &str,
    content: String,
    description: Option<String>,
    project_id: String,
    due: Option<TodoistItemDue>,
    priority: TodoistItemPriority,
) -> Mock<'a> {
    let sync_item_add_todoist_response = TodoistSyncStatusResponse {
        sync_status: HashMap::from([(Uuid::new_v4(), TodoistCommandStatus::Ok("ok".to_string()))]),
        full_sync: false,
        temp_id_mapping: HashMap::from([(Uuid::new_v4().to_string(), new_item_id.to_string())]),
        sync_token: "sync token".to_string(),
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
}

pub fn mock_todoist_get_item_service(
    todoist_mock_server: &MockServer,
    result: Box<TodoistItem>,
) -> Mock {
    let item_id = result.id.clone();
    let response = TodoistItemInfoResponse { item: *result };

    todoist_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/items/get")
            .body(format!("item_id={item_id}&all_data=false"))
            .header("authorization", "Bearer todoist_test_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(&response);
    })
}

pub fn mock_todoist_delete_item_service<'a>(
    todoist_mock_server: &'a MockServer,
    task_id: &str,
) -> Mock<'a> {
    mock_todoist_sync_service(
        todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemDelete {
            args: TodoistSyncCommandItemDeleteArgs {
                id: task_id.to_string(),
            },
        }],
        None,
    )
}

pub fn mock_todoist_complete_item_service<'a>(
    todoist_mock_server: &'a MockServer,
    task_id: &str,
) -> Mock<'a> {
    mock_todoist_sync_service(
        todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemComplete {
            args: TodoistSyncCommandItemCompleteArgs {
                id: task_id.to_string(),
            },
        }],
        None,
    )
}

pub fn mock_todoist_sync_project_add<'a>(
    todoist_mock_server: &'a MockServer,
    new_project: &str,
    new_project_id: &str,
) -> Mock<'a> {
    let sync_project_add_todoist_response = TodoistSyncStatusResponse {
        sync_status: HashMap::from([(Uuid::new_v4(), TodoistCommandStatus::Ok("ok".to_string()))]),
        full_sync: false,
        temp_id_mapping: HashMap::from([(Uuid::new_v4().to_string(), new_project_id.to_string())]),
        sync_token: "sync token".to_string(),
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
}

pub fn mock_todoist_sync_service(
    todoist_mock_server: &MockServer,
    commands: Vec<TodoistSyncPartialCommand>,
    result: Option<TodoistSyncStatusResponse>,
) -> Mock {
    let body = json!({ "commands": commands });

    debug!("Mocking Todoist sync service with body: {}", body);
    let response = result.unwrap_or_else(|| {
        let status: HashMap<Uuid, TodoistCommandStatus> = commands
            .iter()
            .map(|_| (Uuid::new_v4(), TodoistCommandStatus::Ok("ok".to_string())))
            .collect();
        TodoistSyncStatusResponse {
            sync_status: status,
            full_sync: false,
            temp_id_mapping: HashMap::new(),
            sync_token: "sync token".to_string(),
        }
    });

    todoist_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/sync")
            .json_body_partial(body.to_string())
            .header("authorization", "Bearer todoist_test_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(&response);
    })
}

pub fn mock_todoist_sync_resources_service<'a>(
    todoist_mock_server: &'a MockServer,
    resource_name: &str,
    result: &TodoistSyncResponse,
) -> Mock<'a> {
    todoist_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/sync")
            .json_body(json!({ "sync_token": "*", "resource_types": [resource_name] }))
            .header("authorization", "Bearer todoist_test_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

#[fixture]
pub fn todoist_item() -> Box<TodoistItem> {
    load_json_fixture_file("/tests/api/fixtures/todoist_item.json")
}

pub fn assert_sync_items(
    task_creations: &[TaskCreationResult],
    sync_todoist_items: &[TodoistItem],
) {
    for task_creation in task_creations.iter() {
        let task = &task_creation.task;
        let notification = task_creation.notification.clone();

        match task.source_id.as_ref() {
            "1123" => {
                assert_eq!(task.title, "Task 1".to_string());
                assert_eq!(task.status, TaskStatus::Active);
                assert_eq!(
                    task.due_at,
                    Some(DueDate::Date(NaiveDate::from_ymd_opt(2016, 9, 1).unwrap()))
                );
                assert_eq!(
                    task.source_html_url,
                    Some(
                        "https://todoist.com/showTask?id=1123"
                            .parse::<Uri>()
                            .unwrap()
                    )
                );
                assert_eq!(task.project, "Inbox".to_string());
                assert_eq!(
                    task.created_at,
                    Utc.with_ymd_and_hms(2019, 12, 11, 22, 36, 50).unwrap()
                );
                assert_eq!(
                    task.metadata,
                    TaskMetadata::Todoist(sync_todoist_items[0].clone())
                );

                assert!(notification.is_some());
                let notif = notification.unwrap();
                assert_eq!(notif.title, task.title);
                assert_eq!(notif.source_id, task.source_id.clone());
                assert_eq!(notif.status, NotificationStatus::Unread);
                assert_eq!(notif.source_html_url, task.source_html_url);
                assert_eq!(notif.updated_at, task.created_at);
                assert_eq!(notif.metadata, NotificationMetadata::Todoist);
                assert_eq!(notif.task_id, Some(task.id));
            }
            // This task should be updated
            "1456" => {
                assert_eq!(task.title, "Task 2".to_string());
                assert_eq!(task.status, TaskStatus::Active);
                assert_eq!(
                    task.source_html_url,
                    Some(
                        "https://todoist.com/showTask?id=1456"
                            .parse::<Uri>()
                            .unwrap()
                    )
                );
                assert_eq!(task.project, "Project2".to_string());
                assert_eq!(
                    task.created_at,
                    Utc.with_ymd_and_hms(2019, 12, 11, 22, 37, 50).unwrap()
                );
                assert_eq!(
                    task.metadata,
                    TaskMetadata::Todoist(sync_todoist_items[1].clone())
                );
                assert!(notification.is_none());
            }
            _ => {
                unreachable!("Unexpected task title '{}'", &task.title);
            }
        }
    }
}

pub async fn create_task_from_todoist_item(
    client: &Client,
    app_address: &str,
    todoist_item: &TodoistItem,
    project: String,
) -> Box<TaskCreationResult> {
    create_resource(
        client,
        app_address,
        "tasks",
        Box::new(Task {
            id: Uuid::new_v4().into(),
            source_id: todoist_item.id.clone(),
            title: todoist_item.content.clone(),
            body: todoist_item.description.clone(),
            status: if todoist_item.checked {
                TaskStatus::Done
            } else {
                TaskStatus::Active
            },
            completed_at: todoist_item.completed_at,
            priority: todoist_item.priority.into(),
            due_at: todoist_item.due.as_ref().map(|due| due.date.clone()),
            source_html_url: todoist::get_task_html_url(&todoist_item.id),
            tags: todoist_item.labels.clone(),
            parent_id: None,
            project,
            is_recurring: todoist_item
                .due
                .as_ref()
                .map(|due| due.is_recurring)
                .unwrap_or(false),
            created_at: todoist_item.added_at,
            metadata: TaskMetadata::Todoist(todoist_item.clone()),
        }),
    )
    .await
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
