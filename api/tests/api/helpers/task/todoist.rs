use std::{env, fs};

use format_serde_error::SerdeError;
use httpmock::{Method::GET, Mock, MockServer};
use rstest::*;

use universal_inbox::notification::integrations::todoist::TodoistTask;
use universal_inbox::task::integrations::todoist::TodoistTask as TodoistTask2;

use crate::helpers::load_json_fixture_file;

#[fixture]
pub fn sync_todoist_tasks() -> Vec<TodoistTask> {
    load_json_fixture_file("/tests/api/fixtures/sync_todoist_tasks.json")
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

#[fixture]
pub fn todoist_task2() -> Box<TodoistTask2> {
    let fixture_path = format!(
        "{}/tests/api/fixtures/todoist_task.json",
        env::var("CARGO_MANIFEST_DIR").unwrap(),
    );
    let input_str = fs::read_to_string(fixture_path).unwrap();
    serde_json::from_str(&input_str)
        .map_err(|err| SerdeError::new(input_str, err))
        .unwrap()
}
