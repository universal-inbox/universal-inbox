use std::{env, fs};

use chrono::{TimeZone, Utc};
use format_serde_error::SerdeError;
use http::Uri;
use httpmock::{Method::GET, Mock, MockServer};
use rstest::*;

use universal_inbox::{
    integrations::todoist::TodoistTask, Notification, NotificationMetadata, NotificationStatus,
};

use super::load_json_fixture_file;

#[fixture]
pub fn sync_todoist_tasks() -> Vec<TodoistTask> {
    load_json_fixture_file("/tests/api/fixtures/sync_todoist_tasks.json")
}

pub fn assert_sync_notifications(
    notifications: &[Notification],
    sync_todoist_tasks: &[TodoistTask],
) {
    for notification in notifications.iter() {
        match notification.source_id.as_ref() {
            "1123" => {
                assert_eq!(notification.title, "Task 1".to_string());
                assert_eq!(notification.status, NotificationStatus::Unread);
                assert_eq!(
                    notification.source_html_url,
                    Some(
                        "https://todoist.com/showTask?id=1123"
                            .parse::<Uri>()
                            .unwrap()
                    )
                );
                assert_eq!(
                    notification.updated_at,
                    Utc.with_ymd_and_hms(2019, 12, 11, 22, 36, 50).unwrap()
                );
                assert_eq!(
                    notification.metadata,
                    NotificationMetadata::Todoist(sync_todoist_tasks[0].clone())
                );
            }
            // This notification should be updated
            "1456" => {
                assert_eq!(notification.title, "Task 2".to_string());
                assert_eq!(notification.status, NotificationStatus::Unread);
                assert_eq!(
                    notification.source_html_url,
                    Some(
                        "https://todoist.com/showTask?id=1456"
                            .parse::<Uri>()
                            .unwrap()
                    )
                );
                assert_eq!(
                    notification.updated_at,
                    Utc.with_ymd_and_hms(2019, 12, 11, 22, 37, 50).unwrap()
                );
                assert_eq!(
                    notification.metadata,
                    NotificationMetadata::Todoist(sync_todoist_tasks[1].clone())
                );
            }
            _ => {
                unreachable!("Unexpected notification title '{}'", &notification.title);
            }
        }
    }
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
