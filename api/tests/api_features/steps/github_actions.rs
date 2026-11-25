use chrono::{TimeDelta, TimeZone, Utc};
use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

use universal_inbox::{
    notification::{Notification, NotificationStatus, service::NotificationPatch},
    task::Task,
    third_party::{integrations::todoist::TodoistItem, item::ThirdPartyItemData},
};

use crate::{
    helpers::{
        rest::{get_resource, patch_resource_response},
        task::todoist::todoist_item,
        third_party::create_task_third_party_item,
    },
    state::GithubCall,
    steps::notifications::update,
    world::{ApiWorld, Response},
};

const THREAD_PATH: &str = "/notifications/threads/1";

async fn github_answers(world: &mut ApiWorld, mark_as_read_status: u16, delete_status: u16) {
    let app = world.app.get().await;
    let mark_as_read = Mock::given(method("PATCH"))
        .and(path(THREAD_PATH))
        .respond_with(ResponseTemplate::new(mark_as_read_status))
        .mount_as_scoped(&app.github_mock_server)
        .await;
    let delete = Mock::given(method("DELETE"))
        .and(path(THREAD_PATH))
        .respond_with(ResponseTemplate::new(delete_status))
        .mount_as_scoped(&app.github_mock_server)
        .await;
    let calls = &mut world.notifications.github_calls;
    calls.insert(GithubCall::MarkAsRead, mark_as_read);
    calls.insert(GithubCall::Delete, delete);
}

#[given(expr = "GitHub answers {int} when asked to mark the thread as read and to delete it")]
async fn github_answers_to_mark_as_read_and_delete(world: &mut ApiWorld, status: u16) {
    github_answers(world, status, status).await;
}

#[given("GitHub accepts to mark the thread as read and to delete it")]
async fn github_accepts_to_mark_as_read_and_delete(world: &mut ApiWorld) {
    github_answers(world, 205, 200).await;
}

#[given("GitHub would accept to unsubscribe from the thread")]
async fn github_would_accept_to_unsubscribe(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let subscription = Mock::given(method("PUT"))
        .and(path(format!("{THREAD_PATH}/subscription")))
        .respond_with(ResponseTemplate::new(200))
        .mount_as_scoped(&app.github_mock_server)
        .await;
    world
        .notifications
        .github_calls
        .insert(GithubCall::Subscription, subscription);
}

#[then("GitHub was asked once to unsubscribe from the thread")]
async fn github_was_asked_once_to_unsubscribe(world: &mut ApiWorld) {
    let guard = &world.notifications.github_calls[&GithubCall::Subscription];
    assert_eq!(guard.received_requests().await.len(), 1);
}

#[then("GitHub was not asked to unsubscribe from the thread")]
async fn github_was_not_asked_to_unsubscribe(world: &mut ApiWorld) {
    let guard = &world.notifications.github_calls[&GithubCall::Subscription];
    assert_eq!(guard.received_requests().await.len(), 0);
}

#[given("GitHub refuses to delete the thread")]
async fn github_refuses_to_delete_the_thread(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let delete = Mock::given(method("DELETE"))
        .and(path(THREAD_PATH))
        .respond_with(ResponseTemplate::new(403))
        .mount_as_scoped(&app.github_mock_server)
        .await;
    world
        .notifications
        .github_calls
        .insert(GithubCall::Delete, delete);
}

#[given(expr = "I have a Todoist task in the project {string}, linked to {string}")]
#[when(expr = "I have a Todoist task in the project {string}, linked to {string}")]
async fn i_have_a_todoist_task_linked_to(world: &mut ApiWorld, project: String, name: String) {
    let project_id = match project.as_str() {
        "Project2" => "2222",
        other => panic!("Unknown Todoist project {other:?}"),
    };
    let user_id = world.user().id;
    let app = world.app.get().await;
    let creation = create_task_third_party_item(
        app,
        ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
            project_id: project_id.to_string(),
            added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
            ..*todoist_item()
        })),
        user_id,
    )
    .await;
    let task = creation.task.expect("Expected a task to be created");
    update(
        world,
        &name,
        NotificationPatch {
            task_id: Some(task.id),
            ..Default::default()
        },
    )
    .await;
    world.notifications.linked_task = Some(task);
}

async fn mark_as(world: &mut ApiWorld, name: &str, status: NotificationStatus) {
    let id = world.notifications.named(name).id;
    let client = world.client();
    let app = world.app.get().await;
    let response = patch_resource_response(
        &client,
        &app.api_address,
        "notifications",
        id.into(),
        &NotificationPatch {
            status: Some(status),
            ..Default::default()
        },
    )
    .await;
    let recorded = Response::read(response).await;
    if recorded.status.is_success() {
        world.notifications.patched = Some(recorded.json());
    }
    world.http.record(recorded);
}

#[when(expr = "I mark {string} as deleted")]
async fn i_mark_a_notification_as_deleted(world: &mut ApiWorld, name: String) {
    mark_as(world, &name, NotificationStatus::Deleted).await;
}

#[when(expr = "I mark {string} as unsubscribed")]
async fn i_mark_a_notification_as_unsubscribed(world: &mut ApiWorld, name: String) {
    mark_as(world, &name, NotificationStatus::Unsubscribed).await;
}

fn assert_patched_with_read_marker_moved_forward(
    world: &ApiWorld,
    name: &str,
    status: NotificationStatus,
) {
    let original = world.notifications.named(name);
    let patched = world
        .notifications
        .patched
        .as_ref()
        .expect("No notification was patched");
    let original_last_read_at = original
        .last_read_at
        .expect("The notification had a last_read_at");
    let patched_last_read_at = patched
        .last_read_at
        .expect("A dismissed notification has a last_read_at");
    assert!(patched_last_read_at > original_last_read_at);
    assert!(Utc::now() - patched_last_read_at < TimeDelta::minutes(1));
    assert_eq!(
        patched,
        &Notification {
            status,
            last_read_at: patched.last_read_at,
            ..original.clone()
        }
    );
}

#[then(expr = "the patched notification is {string}, deleted, with its read marker moved forward")]
async fn the_patched_notification_is_deleted(world: &mut ApiWorld, name: String) {
    assert_patched_with_read_marker_moved_forward(world, &name, NotificationStatus::Deleted);
    crate::steps::notifications::assert_the_patched_notification_is_stored(world).await;
}

#[then(
    expr = "the patched notification is {string}, unsubscribed, with its read marker moved forward"
)]
async fn the_patched_notification_is_unsubscribed(world: &mut ApiWorld, name: String) {
    assert_patched_with_read_marker_moved_forward(world, &name, NotificationStatus::Unsubscribed);
    crate::steps::notifications::assert_the_patched_notification_is_stored(world).await;
}

#[then("GitHub was asked once to mark the thread as read, and once to delete it")]
async fn github_was_asked_once_to_mark_as_read_and_delete(world: &mut ApiWorld) {
    for call in [GithubCall::MarkAsRead, GithubCall::Delete] {
        let guard = &world.notifications.github_calls[&call];
        assert_eq!(guard.received_requests().await.len(), 1, "{call:?}");
    }
}

#[then("the Todoist task keeps its status")]
async fn the_todoist_task_keeps_its_status(world: &mut ApiWorld) {
    let linked = world
        .notifications
        .linked_task
        .clone()
        .expect("No task is linked to a notification");
    let client = world.client();
    let app = world.app.get().await;
    let task: Box<Task> = get_resource(&client, &app.api_address, "tasks", linked.id.into()).await;
    assert_eq!(task.status, linked.status);
}

#[then("the app says an internal error occurred, and asks to retry later or contact support")]
fn the_app_says_an_internal_error_occurred(world: &mut ApiWorld) {
    let body: serde_json::Value = world.http.last().json();
    assert_eq!(
        body["message"],
        "An internal error occurred. Please retry later or contact support with the correlation id."
    );
}
