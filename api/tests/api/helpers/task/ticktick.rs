use pretty_assertions::assert_eq;
use rstest::*;
use serde_json::json;
use url::Url;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use universal_inbox::{
    HasHtmlUrl,
    notification::{NotificationSourceKind, NotificationStatus},
    task::{DueDate, TaskCreationResult, TaskStatus},
    third_party::{
        integrations::ticktick::{TickTickItem, TickTickItemPriority},
        item::ThirdPartyItemData,
    },
    user::UserId,
};

use universal_inbox::task::integrations::ticktick::TickTickProject;
use universal_inbox_api::integrations::ticktick::TickTickCreateTaskResponse;

use crate::helpers::load_json_fixture_file;

#[fixture]
pub fn ticktick_item() -> Box<TickTickItem> {
    load_json_fixture_file("ticktick_item.json")
}

#[fixture]
pub fn ticktick_projects_response() -> Vec<TickTickProject> {
    load_json_fixture_file("ticktick_projects_response.json")
}

#[fixture]
pub fn ticktick_tasks_response() -> Vec<TickTickItem> {
    load_json_fixture_file("ticktick_tasks_response.json")
}

pub async fn mock_ticktick_list_projects_service(
    ticktick_mock_server: &MockServer,
    result: &[TickTickProject],
) {
    Mock::given(method("GET"))
        .and(path("/project"))
        .and(header("authorization", "Bearer ticktick_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(ticktick_mock_server)
        .await;
}

pub async fn mock_ticktick_list_tasks_service(
    ticktick_mock_server: &MockServer,
    project_id: &str,
    result: &[TickTickItem],
) {
    Mock::given(method("GET"))
        .and(path(format!("/project/{project_id}/task")))
        .and(header("authorization", "Bearer ticktick_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(ticktick_mock_server)
        .await;
}

pub async fn mock_ticktick_get_task_service(
    ticktick_mock_server: &MockServer,
    project_id: &str,
    task_id: &str,
    result: &TickTickItem,
) {
    Mock::given(method("GET"))
        .and(path(format!("/project/{project_id}/task/{task_id}")))
        .and(header("authorization", "Bearer ticktick_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(ticktick_mock_server)
        .await;
}

pub async fn mock_ticktick_create_task_service(
    ticktick_mock_server: &MockServer,
    expected_title: &str,
    expected_project_id: Option<&str>,
    expected_priority: TickTickItemPriority,
    result: &TickTickCreateTaskResponse,
) {
    let mut body = json!({
        "title": expected_title,
        "priority": expected_priority,
    });
    if let Some(project_id) = expected_project_id {
        body.as_object_mut()
            .unwrap()
            .insert("projectId".to_string(), json!(project_id));
    }

    Mock::given(method("POST"))
        .and(path("/task"))
        .and(body_partial_json(body))
        .and(header("authorization", "Bearer ticktick_test_access_token"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(result),
        )
        .mount(ticktick_mock_server)
        .await;
}

pub async fn mock_ticktick_complete_task_service(
    ticktick_mock_server: &MockServer,
    project_id: &str,
    task_id: &str,
) {
    Mock::given(method("POST"))
        .and(path(format!(
            "/project/{project_id}/task/{task_id}/complete"
        )))
        .and(header("authorization", "Bearer ticktick_test_access_token"))
        .respond_with(ResponseTemplate::new(200))
        .mount(ticktick_mock_server)
        .await;
}

pub async fn mock_ticktick_delete_task_service(
    ticktick_mock_server: &MockServer,
    project_id: &str,
    task_id: &str,
) {
    Mock::given(method("DELETE"))
        .and(path(format!("/project/{project_id}/task/{task_id}")))
        .and(header("authorization", "Bearer ticktick_test_access_token"))
        .respond_with(ResponseTemplate::new(200))
        .mount(ticktick_mock_server)
        .await;
}

pub fn assert_sync_ticktick_items(
    task_creations: &[TaskCreationResult],
    ticktick_items: &[TickTickItem],
    expected_user_id: UserId,
) {
    for task_creation in task_creations.iter() {
        let task = &task_creation.task;
        let notification = task_creation.notifications.first();

        assert_eq!(task.user_id, expected_user_id);
        match task.source_item.source_id.as_ref() {
            "tt_task_1123" => {
                assert_eq!(
                    task.title,
                    "Release new version of Universal Inbox".to_string()
                );
                assert_eq!(
                    task.status,
                    if ticktick_items[0].is_completed() {
                        TaskStatus::Done
                    } else {
                        TaskStatus::Active
                    }
                );
                assert_eq!(
                    task.due_at,
                    ticktick_items[0]
                        .get_due_date()
                        .map(|d| DueDate::DateTimeWithTz(match d {
                            DueDate::DateTimeWithTz(dt) => dt,
                            _ => unreachable!(),
                        }))
                );
                assert_eq!(
                    task.get_html_url(),
                    "https://ticktick.com/webapp/#p/tt_proj_1111/tasks/tt_task_1123"
                        .parse::<Url>()
                        .unwrap()
                );
                assert_eq!(task.project, "Inbox".to_string());
                assert_eq!(
                    task.source_item.data,
                    ThirdPartyItemData::TickTickItem(Box::new(ticktick_items[0].clone()))
                );

                // Inbox tasks should have a notification
                assert!(notification.is_some());
                let notif = notification.unwrap();
                assert_eq!(notif.title, task.title);
                assert_eq!(notif.kind, NotificationSourceKind::TickTick);
                assert_eq!(
                    notif.status,
                    if ticktick_items[0].is_completed() {
                        NotificationStatus::Deleted
                    } else {
                        NotificationStatus::Unread
                    }
                );
                assert_eq!(notif.source_item.id, task.source_item.id);
                assert_eq!(notif.task_id, Some(task.id));
                assert_eq!(notif.user_id, expected_user_id);
            }
            "tt_task_1456" => {
                assert_eq!(task.title, "Task 2".to_string());
                assert_eq!(task.status, TaskStatus::Active);
                assert_eq!(
                    task.get_html_url(),
                    "https://ticktick.com/webapp/#p/tt_proj_2222/tasks/tt_task_1456"
                        .parse::<Url>()
                        .unwrap()
                );
                assert_eq!(task.project, "Project2".to_string());
                assert_eq!(
                    task.source_item.data,
                    ThirdPartyItemData::TickTickItem(Box::new(ticktick_items[1].clone()))
                );
                // Non-inbox task notification should be deleted
                assert!(notification.is_some());
                let notif = notification.unwrap();
                assert_eq!(notif.status, NotificationStatus::Deleted);
                assert_eq!(notif.kind, NotificationSourceKind::TickTick);
                assert_eq!(notif.source_item.id, task.source_item.id);
            }
            _ => {
                unreachable!("Unexpected task title '{}'", &task.title);
            }
        }
    }
}
