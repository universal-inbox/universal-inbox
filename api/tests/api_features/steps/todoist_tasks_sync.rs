use std::collections::HashMap;

use chrono::{TimeZone, Utc};
use cucumber::{given, then, when};
use http::StatusCode;
use pretty_assertions::assert_eq;
use wiremock::{Mock, ResponseTemplate, matchers::any};

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::todoist::{SyncToken, TodoistConfig, TodoistContext},
        provider::{IntegrationConnectionContext, IntegrationProvider},
    },
    notification::{Notification, NotificationStatus, NotificationWithTask},
    task::{Task, TaskPriority, TaskSourceKind, TaskStatus},
    third_party::{
        integrations::todoist::{TodoistItem, TodoistItemPriority, TodoistLabel},
        item::ThirdPartyItemData,
    },
};
use universal_inbox_api::{
    integrations::todoist::TodoistSyncResponse,
    repository::integration_connection::IntegrationConnectionRepository,
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            create_and_mock_integration_connection, todoist_oauth_credential,
        },
        notification::list_notifications_with_tasks,
        rest::get_resource,
        task::{
            list_tasks, list_tasks_until, sync_tasks, sync_tasks_response,
            todoist::{
                assert_sync_items, mock_todoist_sync_resources_service,
                sync_todoist_items_response, sync_todoist_projects_response,
            },
        },
        third_party::create_task_third_party_item,
    },
    steps::integration_connections::stored_connection,
    world::ApiWorld,
};

const INBOX_PROJECT_ID: &str = "1111";

#[derive(Clone, Copy, PartialEq, Eq)]
enum SyncMode {
    Foreground,
    Background,
}

async fn connect(world: &mut ApiWorld, config: TodoistConfig) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Todoist(config),
        &settings(),
        todoist_oauth_credential(),
        None,
        None,
    )
    .await;
    world.integrations.todoist_connection = Some(connection);
}

#[given("I am connected to Todoist, without a notification for the tasks of my Inbox")]
async fn connected_without_inbox_notifications(world: &mut ApiWorld) {
    connect(
        world,
        TodoistConfig {
            create_notification_from_inbox_task: false,
            ..TodoistConfig::enabled()
        },
    )
    .await;
}

#[given("I am connected to Todoist, with its synchronization disabled")]
async fn connected_with_the_sync_disabled(world: &mut ApiWorld) {
    connect(world, TodoistConfig::disabled()).await;
}

#[given("I have a Todoist connection that is not validated yet")]
async fn a_connection_not_validated(world: &mut ApiWorld) {
    connect(world, TodoistConfig::enabled()).await;
    let connection = world.integrations.todoist_connection.as_ref().unwrap();
    let (id, user_id) = (connection.id, connection.user_id);
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .update_integration_connection_status(
            &mut transaction,
            id,
            IntegrationConnectionStatus::Created,
            None,
            None,
            user_id,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

#[given("Todoist reports its items")]
fn todoist_reports_its_items(world: &mut ApiWorld) {
    world.todoist_tasks.reported = Some(sync_todoist_items_response());
}

#[given("Todoist reports its items, except the first one")]
fn todoist_reports_its_items_but_the_first(world: &mut ApiWorld) {
    let mut response = sync_todoist_items_response();
    response.items = Some(response.items.unwrap().into_iter().skip(1).collect());
    world.todoist_tasks.reported = Some(response);
}

#[given(expr = "Todoist reports the item {string} as completed")]
fn todoist_reports_an_item_as_completed(world: &mut ApiWorld, source_id: String) {
    let item = world
        .todoist_tasks
        .reported
        .as_mut()
        .expect("Todoist reports no item")
        .items
        .as_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item.id == source_id)
        .expect("Todoist does not report that item");
    item.checked = true;
    item.completed_at = Some(Utc::now());
}

#[given("Todoist reports a single deleted item with no content, attached to no project")]
async fn todoist_reports_an_empty_item(world: &mut ApiWorld) {
    world.todoist_tasks.reported = Some(TodoistSyncResponse {
        items: Some(vec![TodoistItem {
            id: "id1".to_string(),
            parent_id: None,
            project_id: "0".to_string(),
            sync_id: None,
            section_id: None,
            content: "".to_string(),
            description: "".to_string(),
            labels: vec![],
            child_order: 0,
            day_order: Some(-1),
            priority: TodoistItemPriority::P1,
            checked: false,
            is_deleted: true,
            collapsed: false,
            completed_at: None,
            added_at: Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap(),
            due: None,
            user_id: "11".to_string(),
            added_by_uid: None,
            assigned_by_uid: None,
            responsible_uid: None,
        }]),
        projects: None,
        labels: None,
        full_sync: true,
        temp_id_mapping: HashMap::new(),
        sync_token: SyncToken("sync_token".to_string()),
    });
    let app = world.app.get().await;
    mock_todoist_sync_resources_service(
        &app.todoist_mock_server,
        "projects",
        &TodoistSyncResponse {
            items: None,
            projects: Some(vec![]),
            labels: None,
            full_sync: true,
            temp_id_mapping: HashMap::new(),
            sync_token: SyncToken("project_sync_token".to_string()),
        },
        None,
    )
    .await;
}

#[given(expr = "the sync token of my Todoist connection is {string}")]
async fn the_sync_token_is(world: &mut ApiWorld, token: String) {
    let id = world.integrations.todoist_connection.as_ref().unwrap().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .update_integration_connection_context(
            &mut transaction,
            id,
            Some(IntegrationConnectionContext::Todoist(TodoistContext {
                items_sync_token: SyncToken(token.clone()),
            })),
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    world.todoist_tasks.expected_sync_token = Some(SyncToken(token));
}

#[given(
    expr = "Todoist reports no change since that sync token, and gives the new sync token {string}"
)]
fn todoist_reports_no_change(world: &mut ApiWorld, new_token: String) {
    world.todoist_tasks.reported = Some(TodoistSyncResponse {
        items: Some(vec![]),
        projects: None,
        labels: None,
        full_sync: false,
        temp_id_mapping: HashMap::new(),
        sync_token: SyncToken(new_token),
    });
}

#[given("Todoist answers every request with an error")]
async fn todoist_answers_with_an_error(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(400))
        .mount(&app.todoist_mock_server)
        .await;
}

#[given("Todoist answers every request with a success and no data")]
#[when("Todoist answers every request with a success and no data")]
async fn todoist_answers_with_nothing(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.todoist_mock_server)
        .await;
}

#[given(expr = "I already have the task {string} of the second Todoist item, in my Inbox")]
async fn i_already_have_the_task_of_the_second_item(world: &mut ApiWorld, title: String) {
    let user_id = world.user().id;
    let items = sync_todoist_items_response().items.unwrap();
    let app = world.app.get().await;
    let creation = create_task_third_party_item(
        app,
        ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
            content: title,
            description: "more details".to_string(),
            checked: false,
            is_deleted: false,
            completed_at: None,
            priority: TodoistItemPriority::P4,
            due: None,
            labels: vec![TodoistLabel::from_name("tag1")],
            parent_id: None,
            project_id: INBOX_PROJECT_ID.to_string(),
            ..items[1].clone()
        })),
        user_id,
    )
    .await;
    world.todoist_tasks.known.push(*creation);
}

#[given("I already have the tasks of every Todoist item, in my Inbox")]
async fn i_already_have_every_task(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let items = sync_todoist_items_response().items.unwrap();
    for item in items {
        let app = world.app.get().await;
        let creation = create_task_third_party_item(
            app,
            ThirdPartyItemData::TodoistItem(Box::new(TodoistItem {
                project_id: INBOX_PROJECT_ID.to_string(),
                ..item
            })),
            user_id,
        )
        .await;
        world.todoist_tasks.known.push(*creation);
    }
}

async fn mount_items(world: &mut ApiWorld) {
    let reported = world.todoist_tasks.reported.clone();
    let token = world.todoist_tasks.expected_sync_token.clone();
    let app = world.app.get().await;
    if let Some(response) = reported {
        mock_todoist_sync_resources_service(&app.todoist_mock_server, "items", &response, token)
            .await;
    }
}

#[when("I sync the tasks of my Todoist connection")]
async fn i_sync_my_todoist_tasks(world: &mut ApiWorld) {
    mount_items(world).await;
    let client = world.client();
    let app = world.app.get().await;
    world.todoist_tasks.results = sync_tasks(
        &client,
        &app.api_address,
        Some(TaskSourceKind::Todoist),
        false,
    )
    .await;
}

#[when("I ask for a synchronous sync of my Todoist tasks")]
async fn i_ask_for_a_sync(world: &mut ApiWorld) {
    request_sync(world, SyncMode::Foreground).await;
}

#[when("I ask for a background sync of my Todoist tasks")]
async fn i_ask_for_a_background_sync(world: &mut ApiWorld) {
    request_sync(world, SyncMode::Background).await;
}

async fn request_sync(world: &mut ApiWorld, mode: SyncMode) {
    mount_items(world).await;
    let client = world.client();
    let app = world.app.get().await;
    let response = sync_tasks_response(
        &client,
        &app.api_address,
        Some(TaskSourceKind::Todoist),
        mode == SyncMode::Background,
    )
    .await;
    world.todoist_tasks.status = Some(response.status());
}

#[when("I list my active tasks, which triggers a sync")]
async fn i_list_my_tasks_triggering_a_sync(world: &mut ApiWorld) {
    mount_items(world).await;
    let client = world.client();
    let app = world.app.get().await;
    world.todoist_tasks.listed = list_tasks(&client, &app.api_address, TaskStatus::Active, true)
        .await
        .len();
}

#[when("I list the active tasks I have")]
async fn i_list_my_active_tasks(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    world.todoist_tasks.listed = list_tasks(&client, &app.api_address, TaskStatus::Active, false)
        .await
        .len();
}

#[when(expr = "I wait until {int} active tasks are listed")]
async fn i_wait_for_active_tasks(world: &mut ApiWorld, count: usize) {
    let client = world.client();
    let app = world.app.get().await;
    list_tasks_until(&client, &app.api_address, TaskStatus::Active, count).await;
}

#[then(expr = "I have {int} active task(s)")]
fn i_have_active_tasks(world: &mut ApiWorld, count: usize) {
    assert_eq!(world.todoist_tasks.listed, count);
}

#[then("the sync request is accepted")]
fn the_sync_request_is_accepted(world: &mut ApiWorld) {
    assert_eq!(world.todoist_tasks.status, Some(StatusCode::OK));
}

#[then("the sync is accepted to run in the background")]
fn the_sync_is_accepted_in_the_background(world: &mut ApiWorld) {
    assert_eq!(world.todoist_tasks.status, Some(StatusCode::CREATED));
}

#[then("every Todoist item became a task")]
fn every_item_became_a_task(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let items = world
        .todoist_tasks
        .reported
        .as_ref()
        .unwrap()
        .items
        .clone()
        .unwrap();
    assert_eq!(world.todoist_tasks.results.len(), items.len());
    assert_sync_items(&world.todoist_tasks.results, &items, user_id);
}

#[then("the sync created no task")]
fn the_sync_created_no_task(world: &mut ApiWorld) {
    assert!(world.todoist_tasks.results.is_empty());
}

async fn task_of_known(world: &mut ApiWorld, index: usize) -> Box<Task> {
    let id = world.todoist_tasks.known[index].task.as_ref().unwrap().id;
    let client = world.client();
    let app = world.app.get().await;
    get_resource(&client, &app.api_address, "tasks", id.into()).await
}

async fn notification_of_known(world: &mut ApiWorld, index: usize) -> Box<NotificationWithTask> {
    let id = world.todoist_tasks.known[index]
        .notification
        .as_ref()
        .unwrap()
        .id;
    let client = world.client();
    let app = world.app.get().await;
    get_resource(&client, &app.api_address, "notifications", id.into()).await
}

#[then("the task I already had was updated from the second Todoist item")]
async fn the_known_task_was_updated(world: &mut ApiWorld) {
    let second = sync_todoist_items_response().items.unwrap()[1].clone();
    let known_source_id = world.todoist_tasks.known[0]
        .task
        .as_ref()
        .unwrap()
        .source_item
        .source_id
        .clone();
    let task = task_of_known(world, 0).await;
    assert_eq!(task.source_item.source_id, known_source_id);
    let project = sync_todoist_projects_response()
        .projects
        .unwrap()
        .into_iter()
        .find(|project| project.id == second.project_id)
        .expect("The second item has no project");
    assert_eq!(task.title, second.content);
    assert_eq!(task.body, second.description);
    assert_eq!(task.priority, second.priority.into());
    assert_eq!(
        task.tags,
        second
            .labels
            .iter()
            .map(|label| label.name.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(task.project, project.name);
    assert_eq!(task.created_at, second.added_at);
    assert_eq!(
        task.source_item.data,
        ThirdPartyItemData::TodoistItem(Box::new(second))
    );
}

#[then(
    expr = "its notification was deleted, as the task left my Inbox, and is still titled {string}"
)]
async fn its_notification_was_deleted(world: &mut ApiWorld, title: String) {
    let task_id = world.todoist_tasks.known[0].task.as_ref().unwrap().id;
    let known_source_id = world.todoist_tasks.known[0]
        .notification
        .as_ref()
        .unwrap()
        .source_item
        .source_id
        .clone();
    let notification = notification_of_known(world, 0).await;
    assert_eq!(
        notification.task.as_ref().map(|task| task.id),
        Some(task_id)
    );
    assert_eq!(notification.source_item.source_id, known_source_id);
    assert_eq!(notification.status, NotificationStatus::Deleted);
    assert_eq!(notification.title, title);
}

#[then("the new task of the first Todoist item, which is in my Inbox, has an unread notification")]
async fn the_new_task_has_a_notification(world: &mut ApiWorld) {
    let first_id = world
        .todoist_tasks
        .reported
        .as_ref()
        .unwrap()
        .items
        .as_ref()
        .unwrap()[0]
        .id
        .clone();
    let creation = world
        .todoist_tasks
        .results
        .iter()
        .find(|creation| creation.task.source_item.source_id == first_id)
        .unwrap()
        .clone();
    let client = world.client();
    let app = world.app.get().await;
    let notifications = list_notifications_with_tasks(
        &client,
        &app.api_address,
        vec![NotificationStatus::Unread],
        false,
        Some(creation.task.id),
        None,
        false,
    )
    .await;
    assert_eq!(notifications.len(), 1);
    assert_eq!(
        notifications[0].source_item.source_id,
        creation.task.source_item.source_id
    );
    assert_eq!(notifications[0].task, Some(creation.task.clone()));
    assert_eq!(
        Into::<Notification>::into(notifications[0].clone()),
        creation.notifications[0]
    );
}

#[then("my Todoist connection kept its configuration and recorded the sync token of that sync")]
async fn the_connection_recorded_the_token(world: &mut ApiWorld) {
    let id = world.integrations.todoist_connection.as_ref().unwrap().id;
    let expected = world
        .todoist_tasks
        .reported
        .as_ref()
        .unwrap()
        .sync_token
        .clone();
    let connection = stored_connection(world, id).await;
    assert_eq!(
        connection.provider,
        IntegrationProvider::Todoist {
            context: Some(TodoistContext {
                items_sync_token: expected
            }),
            config: TodoistConfig::enabled()
        }
    );
}

#[then("my Todoist connection recorded a completed tasks sync, without failure")]
async fn the_connection_recorded_a_completed_sync(world: &mut ApiWorld) {
    let id = world.integrations.todoist_connection.as_ref().unwrap().id;
    let connection = stored_connection(world, id).await;
    assert!(connection.last_tasks_sync_started_at.is_some());
    assert!(connection.last_tasks_sync_completed_at.is_some());
    assert!(connection.last_tasks_sync_failed_at.is_none());
    assert!(connection.last_tasks_sync_failure_message.is_none());
    assert_eq!(connection.tasks_sync_failures, 0);
    assert_eq!(connection.status, IntegrationConnectionStatus::Validated);
    assert!(connection.failure_message.is_none());
}

#[then(expr = "my Todoist connection recorded the failure {string}")]
async fn the_connection_recorded_a_failure(world: &mut ApiWorld, message: String) {
    let id = world.integrations.todoist_connection.as_ref().unwrap().id;
    let connection = stored_connection(world, id).await;
    assert_eq!(
        connection.last_tasks_sync_failure_message.unwrap().as_str(),
        message
    );
}

#[then("no notification was created")]
async fn no_notification_was_created(world: &mut ApiWorld) {
    assert!(
        world
            .todoist_tasks
            .results
            .iter()
            .all(|creation| creation.notifications.is_empty())
    );
    let client = world.client();
    let app = world.app.get().await;
    let notifications = list_notifications_with_tasks(
        &client,
        &app.api_address,
        vec![NotificationStatus::Unread],
        false,
        None,
        None,
        false,
    )
    .await;
    assert_eq!(notifications.len(), 0);
}

#[then("that task is a deleted task with no content, attached to no project")]
fn the_task_is_an_empty_deleted_task(world: &mut ApiWorld) {
    assert_eq!(world.todoist_tasks.results.len(), 1);
    let creation = &world.todoist_tasks.results[0];
    assert!(creation.notifications.is_empty());
    let task = &creation.task;
    assert_eq!(task.body, "".to_string());
    assert!(task.completed_at.is_none());
    assert!(task.due_at.is_none());
    assert!(!task.is_recurring);
    assert!(task.parent_id.is_none());
    assert_eq!(task.priority, TaskPriority::P4);
    assert_eq!(task.project, "No project".to_string());
    assert_eq!(task.source_item.source_id, "id1".to_string());
    assert_eq!(task.status, TaskStatus::Deleted);
    assert!(task.tags.is_empty());
    assert_eq!(task.title, "".to_string());
}

#[then(expr = "the task of the Todoist item {string} is done, and its notification deleted")]
async fn the_task_is_done_and_its_notification_deleted(world: &mut ApiWorld, source_id: String) {
    let index = world
        .todoist_tasks
        .known
        .iter()
        .position(|creation| creation.third_party_item.source_id == source_id)
        .unwrap();
    let task = task_of_known(world, index).await;
    assert_eq!(task.status, TaskStatus::Done);
    assert!(task.completed_at.is_some());
    let task_id = task.id;
    let notification = notification_of_known(world, index).await;
    assert_eq!(
        notification.task.as_ref().map(|task| task.id),
        Some(task_id)
    );
    assert_eq!(notification.status, NotificationStatus::Deleted);
}

#[then("the sync returned the second task only, with a notification, and that task is active")]
fn the_sync_returned_the_second_task_only(world: &mut ApiWorld) {
    let items = world
        .todoist_tasks
        .reported
        .as_ref()
        .unwrap()
        .items
        .clone()
        .unwrap();
    let results = &world.todoist_tasks.results;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].task.source_item.source_id, items[0].id);
    assert_eq!(results[0].task.status, TaskStatus::Active);
    assert_eq!(results[0].notifications.len(), 1);
}

#[then("the first task and its notification were not updated")]
async fn the_first_task_was_not_updated(world: &mut ApiWorld) {
    let task = task_of_known(world, 0).await;
    assert_eq!(task.status, TaskStatus::Active);
    assert!(task.completed_at.is_none());
    let task_id = task.id;
    let notification = notification_of_known(world, 0).await;
    assert_eq!(
        notification.task.as_ref().map(|task| task.id),
        Some(task_id)
    );
    assert_eq!(notification.status, NotificationStatus::Unread);
}

#[then("the sync returned one task for each Todoist item")]
fn the_sync_returned_one_task_per_item(world: &mut ApiWorld) {
    let items = world
        .todoist_tasks
        .reported
        .as_ref()
        .unwrap()
        .items
        .clone()
        .unwrap();
    assert_eq!(world.todoist_tasks.results.len(), items.len());
}

async fn todoist_requests(world: &mut ApiWorld) -> usize {
    let app = world.app.get().await;
    app.todoist_mock_server
        .received_requests()
        .await
        .unwrap()
        .len()
}

#[given("I note how many requests Todoist received")]
#[when("I note how many requests Todoist received")]
async fn i_note_the_requests(world: &mut ApiWorld) {
    let requests = todoist_requests(world).await;
    assert!(requests > 0, "Todoist received no request yet");
    world.todoist_tasks.requests_noted = Some(requests);
}

#[then("Todoist received no other request")]
async fn todoist_received_no_other_request(world: &mut ApiWorld) {
    let noted = world
        .todoist_tasks
        .requests_noted
        .expect("No request count was noted");
    assert_eq!(todoist_requests(world).await, noted);
}

#[then("Todoist received no request")]
async fn todoist_received_no_request(world: &mut ApiWorld) {
    assert_eq!(todoist_requests(world).await, 0);
}
