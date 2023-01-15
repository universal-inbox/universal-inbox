use chrono::{TimeZone, Utc};
use http::Uri;
use httpmock::{Method::POST, Mock, MockServer};
use pretty_assertions::assert_eq;
use rstest::*;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::{
    notification::{NotificationMetadata, NotificationStatus},
    task::{
        integrations::todoist::{self, TodoistItem},
        Task, TaskMetadata, TaskStatus,
    },
};

use universal_inbox_api::{
    integrations::todoist::{TodoistSyncResponse, TodoistSyncStatusResponse},
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

pub fn mock_todoist_sync_items_service<'a>(
    todoist_mock_server: &'a MockServer,
    result: &'a TodoistSyncResponse,
) -> Mock<'a> {
    todoist_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/sync")
            .json_body(json!({ "sync_token": "*", "resource_types": ["items"] }))
            .header("authorization", "Bearer todoist_test_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

pub fn mock_todoist_delete_item_service<'a>(
    todoist_mock_server: &'a MockServer,
    task_id: &'a str,
    result: &'a TodoistSyncStatusResponse,
) -> Mock<'a> {
    todoist_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/sync")
            .json_body_partial(format!(
                r#"{{ "commands": [{{ "type": "item_delete", "args": {{ "id": "{task_id}" }} }}] }}"#
            ))
            .header("authorization", "Bearer todoist_test_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

pub fn mock_todoist_complete_item_service<'a>(
    todoist_mock_server: &'a MockServer,
    task_id: &'a str,
    result: &'a TodoistSyncStatusResponse,
) -> Mock<'a> {
    todoist_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/sync")
            .json_body_partial(format!(
                r#"{{ "commands": [{{ "type": "item_complete", "args": {{ "id": "{task_id}" }} }}] }}"#
            ))
            .header("authorization", "Bearer todoist_test_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

pub fn mock_todoist_sync_projects_service<'a>(
    todoist_mock_server: &'a MockServer,
    result: &'a TodoistSyncResponse,
) -> Mock<'a> {
    todoist_mock_server.mock(|when, then| {
        when.method(POST)
            .path("/sync")
            .json_body(json!({ "sync_token": "*", "resource_types": ["projects"] }))
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
                assert_eq!(notif.task_source_id, Some(task.source_id.clone()));
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
    app_address: &str,
    todoist_item: &TodoistItem,
) -> Box<TaskCreationResult> {
    create_resource(
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
            project: "Inbox".to_string(),
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
