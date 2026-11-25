use apalis::prelude::Storage;
use chrono::DateTime;
use cucumber::{gherkin::Step, given, then, when};
use pretty_assertions::assert_eq;
use serde_json::{Value, json};
use std::time::Duration;
use tokio::time::{Instant, sleep};
use uuid::Uuid;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use universal_inbox::{
    HasHtmlUrl,
    notification::{NotificationStatus, service::NotificationPatch},
};
use universal_inbox_api::repository::notification::NotificationRepository;

use crate::{
    helpers::{
        notification::github::{
            create_notification_from_github_notification, sync_github_notifications,
        },
        task::todoist::{
            mock_todoist_complete_item_service, mock_todoist_get_item_service,
            mock_todoist_item_add_service, todoist_item,
        },
    },
    steps::mcp_protocol::{body, call_tool_in_a_new_session},
    world::ApiWorld,
};

const UNKNOWN: &str = "unknown";

fn notification_id(world: &ApiWorld, source_id: &str) -> Uuid {
    if source_id == UNKNOWN {
        return Uuid::nil();
    }
    world
        .mcp
        .notifications
        .iter()
        .find(|(id, _)| id == source_id)
        .unwrap_or_else(|| panic!("No notification has the source id {source_id}"))
        .1
        .id
        .0
}

#[given(expr = "I have a GitHub notification with the source id {string}")]
async fn i_have_a_notification(world: &mut ApiWorld, source_id: String) {
    let user_id = world.user().id;
    let connection_id = world
        .integrations
        .github_connection
        .as_ref()
        .expect("The account is not connected to GitHub")
        .id;
    let mut source = sync_github_notifications()[0].clone();
    source.id = source_id.clone();
    let app = world.app.get().await;
    let notification =
        create_notification_from_github_notification(app, &source, user_id, connection_id).await;
    world.mcp.notifications.push((source_id, *notification));
}

#[given(expr = "I already read the GitHub notification {string}")]
async fn i_already_read(world: &mut ApiWorld, source_id: String) {
    let id = notification_id(world, &source_id).into();
    let user_id = world.user().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .update_notification(
            &mut transaction,
            id,
            &NotificationPatch {
                status: Some(NotificationStatus::Read),
                ..Default::default()
            },
            user_id,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

#[given(expr = "GitHub accepts to mark the thread {string} as done, once")]
async fn github_accepts_to_mark_as_done(world: &mut ApiWorld, thread: String) {
    let app = world.app.get().await;
    world
        .mcp
        .expected_github_calls
        .push(("DELETE", format!("/notifications/threads/{thread}")));
    Mock::given(method("DELETE"))
        .and(path(format!("/notifications/threads/{thread}")))
        .respond_with(ResponseTemplate::new(205))
        .expect(1)
        .mount(&app.github_mock_server)
        .await;
}

#[given(expr = "GitHub accepts to ignore the thread {string}, once")]
async fn github_accepts_to_ignore(world: &mut ApiWorld, thread: String) {
    let app = world.app.get().await;
    world.mcp.expected_github_calls.push((
        "PUT",
        format!("/notifications/threads/{thread}/subscription"),
    ));
    Mock::given(method("PUT"))
        .and(path(format!(
            "/notifications/threads/{thread}/subscription"
        )))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.github_mock_server)
        .await;
}

#[given(expr = "Todoist creates a task for the notification {string}")]
async fn todoist_creates_a_task(world: &mut ApiWorld, source_id: String) {
    let notification = &world
        .mcp
        .notifications
        .iter()
        .find(|(source, _)| *source == source_id)
        .unwrap()
        .1;
    let title = notification.title.clone();
    let description = format!(
        "- [{}]({})",
        notification.title,
        notification.get_html_url()
    );
    let item = todoist_item();
    let app = world.app.get().await;
    mock_todoist_item_add_service(
        &app.todoist_mock_server,
        &item.id,
        title,
        Some(description),
        None,
        None,
        item.priority,
    )
    .await;
    mock_todoist_get_item_service(&app.todoist_mock_server, item.clone()).await;
    world.mcp.todoist_item = Some(item);
}

#[given("Todoist completes that task")]
async fn todoist_completes_that_task(world: &mut ApiWorld) {
    let item_id = world.mcp.todoist_item.as_ref().unwrap().id.clone();
    let app = world.app.get().await;
    mock_todoist_complete_item_service(&app.todoist_mock_server, &item_id).await;
}

#[when("I list my notifications through MCP, without syncing")]
async fn i_list_my_notifications(world: &mut ApiWorld) {
    call_tool_in_a_new_session(
        world,
        "list_notifications",
        json!({ "trigger_sync": false }),
    )
    .await;
}

#[when("I unsubscribe through MCP from every GitHub notification that I read")]
async fn i_unsubscribe_from_read_ones(world: &mut ApiWorld) {
    call_tool_in_a_new_session(
        world,
        "bulk_act_notifications",
        json!({
            "mode": "filter",
            "statuses": ["Read"],
            "sources": ["Github"],
            "action": "unsubscribe"
        }),
    )
    .await;
}

#[when(
    expr = "I create through MCP a task from the notification {string}, with the priority {int}"
)]
async fn i_create_a_task(world: &mut ApiWorld, source_id: String, priority: u8) {
    let id = notification_id(world, &source_id);
    let title = world
        .mcp
        .notifications
        .iter()
        .find(|(source, _)| *source == source_id)
        .unwrap()
        .1
        .title
        .clone();
    call_tool_in_a_new_session(
        world,
        "create_task_from_notification",
        json!({
            "notification_id": id,
            "task_creation": { "title": title, "priority": priority }
        }),
    )
    .await;
    world.mcp.task_id = body(world)["result"]["structuredContent"]["notification"]["task"]["id"]
        .as_str()
        .map(str::to_string);
}

#[when("I list my active tasks through MCP")]
async fn i_list_my_active_tasks(world: &mut ApiWorld) {
    call_tool_in_a_new_session(
        world,
        "list_tasks",
        json!({ "status": "Active", "only_synced_tasks": false }),
    )
    .await;
}

#[when("I mark that task as done through MCP")]
async fn i_mark_the_task_as_done(world: &mut ApiWorld) {
    let task_id = world.mcp.task_id.clone().expect("No task was created");
    call_tool_in_a_new_session(
        world,
        "update_task",
        json!({ "task_id": task_id, "patch": { "status": "Done" } }),
    )
    .await;
}

#[when("I act through MCP on this list of notifications:")]
async fn i_act_on_a_list(world: &mut ApiWorld, step: &Step) {
    let rows = &step.table.as_ref().expect("A table of actions").rows;
    let entries: Vec<Value> = rows
        .iter()
        .skip(1)
        .map(|row| {
            let mut entry = json!({
                "notification_id": notification_id(world, &row[0]),
                "action": row[1],
            });
            if !row[2].is_empty() {
                entry["snoozed_until"] = json!(row[2].parse::<DateTime<chrono::Utc>>().unwrap());
            }
            entry
        })
        .collect();
    call_tool_in_a_new_session(
        world,
        "bulk_act_notifications",
        json!({ "mode": "list", "notifications": entries }),
    )
    .await;
}

fn content_len(world: &ApiWorld) -> usize {
    body(world)["result"]["structuredContent"]["content"]
        .as_array()
        .unwrap_or_else(|| panic!("Expected a page of content, got: {}", body(world)))
        .len()
}

#[then(regex = r"^the MCP answer lists (\d+) (?:notifications?|tasks?)$")]
fn the_answer_lists_items(world: &mut ApiWorld, count: usize) {
    assert_eq!(body(world)["result"]["isError"], false);
    assert_eq!(content_len(world), count);
}

#[then(expr = "that call changed {int} notification(s)")]
fn the_call_changed(world: &mut ApiWorld, count: usize) {
    let result = &body(world)["result"];
    assert_eq!(result["isError"], false, "{}", body(world));
    assert_eq!(result["structuredContent"]["count"], count);
    assert_eq!(
        result["structuredContent"]["notifications"]
            .as_array()
            .expect("Expected updated notifications")
            .len(),
        count
    );
}

fn changed<'a>(world: &'a ApiWorld, source_id: &str) -> Option<&'a Value> {
    let id = json!(notification_id(world, source_id));
    body(world)["result"]["structuredContent"]["notifications"]
        .as_array()?
        .iter()
        .find(|updated| updated["id"] == id)
}

#[then(expr = "that call left the notification {string} {string}")]
fn the_call_left_the_notification(world: &mut ApiWorld, source_id: String, status: String) {
    let updated = changed(world, &source_id)
        .unwrap_or_else(|| panic!("Notification {source_id} was not updated"));
    assert_eq!(updated["status"], status);
}

#[then(expr = "that call snoozed the notification {string} until {string}")]
fn the_call_snoozed(world: &mut ApiWorld, source_id: String, until: String) {
    let updated = changed(world, &source_id)
        .unwrap_or_else(|| panic!("Notification {source_id} was not updated"));
    assert_eq!(
        updated["snoozed_until"],
        json!(until.parse::<DateTime<chrono::Utc>>().unwrap())
    );
}

#[then("that call did not touch the unknown notification")]
fn the_call_did_not_touch_the_unknown_one(world: &mut ApiWorld) {
    assert!(changed(world, UNKNOWN).is_none());
}

#[then(expr = "that call unsubscribed the notification {string}")]
fn the_call_unsubscribed(world: &mut ApiWorld, source_id: String) {
    let updated = changed(world, &source_id)
        .unwrap_or_else(|| panic!("Notification {source_id} was not updated"));
    assert_eq!(updated["status"], "Unsubscribed");
}

#[then("the MCP answer shows that task as done")]
fn that_task_is_done(world: &mut ApiWorld) {
    assert_eq!(body(world)["result"]["isError"], false, "{}", body(world));
    assert_eq!(body(world)["result"]["structuredContent"]["status"], "Done");
}

#[then("the task was created")]
fn the_task_was_created(world: &mut ApiWorld) {
    assert_eq!(body(world)["result"]["isError"], false, "{}", body(world));
    assert!(world.mcp.task_id.is_some(), "{}", body(world));
}

#[then(
    "GitHub received the calls it was told to expect, and no job is left in the queue, waiting up to 10 seconds"
)]
async fn github_received_the_calls(world: &mut ApiWorld) {
    let expected = world.mcp.expected_github_calls.clone();
    let app = world.app.get().await;
    let mut storage = app.redis_storage.clone();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let received = app
            .github_mock_server
            .received_requests()
            .await
            .expect("Request recording is enabled");
        let missing: Vec<_> = expected
            .iter()
            .filter(|(verb, path)| {
                !received
                    .iter()
                    .any(|request| request.method.as_str() == *verb && request.url.path() == path)
            })
            .collect();
        let queued = storage.len().await.expect("Failed to count the jobs");
        if missing.is_empty() && queued == 0 {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "GitHub never received {missing:?}, {queued} job(s) still queued"
        );
        sleep(Duration::from_millis(100)).await;
    }
}
