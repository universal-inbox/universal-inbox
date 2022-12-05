use std::{env, fs};

use chrono::{TimeZone, Utc};
use format_serde_error::SerdeError;
use http::Uri;
use httpmock::{Method::GET, Method::POST, Mock, MockServer};
use rstest::*;
use serde_json::json;

use universal_inbox::{
    notification::integrations::todoist::TodoistTask,
    task::{
        integrations::todoist::{self, TodoistItem, TodoistProject},
        Task, TaskMetadata, TaskStatus,
    },
};

use crate::helpers::{load_json_fixture_file, rest::create_resource};

#[fixture]
pub fn sync_todoist_tasks() -> Vec<TodoistTask> {
    load_json_fixture_file("/tests/api/fixtures/sync_todoist_tasks.json")
}

#[fixture]
pub fn sync_todoist_items() -> Vec<TodoistItem> {
    load_json_fixture_file("/tests/api/fixtures/sync_todoist_items.json")
}

#[fixture]
pub fn sync_todoist_projects() -> Vec<TodoistProject> {
    load_json_fixture_file("/tests/api/fixtures/sync_todoist_projects.json")
}

pub fn mock_todoist_tasks_service<'a>(
    todoist_mock_server: &'a MockServer,
    result: &'a Vec<TodoistTask>,
) -> Mock<'a> {
    todoist_mock_server.mock(|when, then| {
        when.method(GET)
            .path("/tasks")
            .query_param("filter", "#Inbox")
            .header("authorization", "Bearer todoist_test_token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body_obj(result);
    })
}

pub fn mock_todoist_sync_items_service<'a>(
    todoist_mock_server: &'a MockServer,
    result: &'a Vec<TodoistItem>,
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

pub fn mock_todoist_sync_projects_service<'a>(
    todoist_mock_server: &'a MockServer,
    result: &'a Vec<TodoistProject>,
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
pub fn todoist_task() -> Box<TodoistTask> {
    let fixture_path = format!(
        "{}/tests/api/fixtures/todoist_task.json",
        env::var("CARGO_MANIFEST_DIR").unwrap(),
    );
    let input_str = fs::read_to_string(fixture_path).unwrap();
    serde_json::from_str(&input_str)
        .map_err(|err| SerdeError::new(input_str, err))
        .unwrap()
}

#[fixture]
pub fn todoist_item() -> Box<TodoistItem> {
    load_json_fixture_file("/tests/api/fixtures/todoist_item.json")
}

pub fn assert_sync_items(tasks: &[Task], sync_todoist_items: &[TodoistItem]) {
    for task in tasks.iter() {
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
) -> Box<Task> {
    create_resource(
        app_address,
        "tasks",
        Box::new(Task {
            id: uuid::Uuid::new_v4(),
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
