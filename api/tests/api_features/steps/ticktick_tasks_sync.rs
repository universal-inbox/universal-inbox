use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use wiremock::{Mock, ResponseTemplate, matchers::any};

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionStatus, config::IntegrationConnectionConfig,
        integrations::ticktick::TickTickConfig, provider::IntegrationProvider,
    },
    notification::{Notification, NotificationStatus, NotificationWithTask},
    task::{ProjectSummary, Task, TaskSourceKind},
    third_party::{
        integrations::{
            linear::LinearIssue,
            ticktick::{TickTickItem, TickTickItemPriority, TickTickTag},
        },
        item::ThirdPartyItemData,
    },
};
use universal_inbox_api::repository::integration_connection::IntegrationConnectionRepository;

use crate::{
    common::settings,
    helpers::{
        integration_connection::create_ticktick_integration_connection,
        notification::{linear::sync_linear_tasks_response, list_notifications_with_tasks},
        rest::get_resource,
        task::{
            get_task,
            linear::create_linear_task_with_ticktick_sink,
            sync_tasks, sync_tasks_response,
            ticktick::{
                assert_sync_ticktick_items, mock_ticktick_inbox_data_service,
                mock_ticktick_list_projects_service, mock_ticktick_list_tasks_service,
                ticktick_projects_response, ticktick_tasks_response,
            },
        },
        third_party::create_task_third_party_item,
    },
    steps::integration_connections::stored_connection,
    world::ApiWorld,
};

const INBOX_PROJECT_ID: &str = "tt_proj_1111";
const PROJECT_2_ID: &str = "tt_proj_2222";
const MIRRORING_ITEM_ID: &str = "tt_task_9999";

async fn connect(world: &mut ApiWorld, config: TickTickConfig) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_ticktick_integration_connection(
        app,
        user_id,
        &settings(),
        IntegrationConnectionConfig::TickTick(config),
        None,
    )
    .await;
    world.integrations.ticktick_connection = Some(connection);
}

#[given("I am connected to TickTick, syncing its tasks")]
async fn connected(world: &mut ApiWorld) {
    connect(world, TickTickConfig::enabled()).await;
    let app = world.app.get().await;
    mock_ticktick_list_projects_service(&app.ticktick_mock_server, &ticktick_projects_response())
        .await;
}

#[given("I am connected to TickTick, with its synchronization disabled")]
async fn connected_with_the_sync_disabled(world: &mut ApiWorld) {
    connect(world, TickTickConfig::disabled()).await;
}

#[given("I have a TickTick connection that is not validated yet")]
async fn a_connection_not_validated(world: &mut ApiWorld) {
    connect(world, TickTickConfig::enabled()).await;
    let connection = world.integrations.ticktick_connection.as_ref().unwrap();
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

#[given("TickTick answers every request with a success and no data")]
async fn ticktick_answers_with_nothing(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.ticktick_mock_server)
        .await;
}

#[given("TickTick reports its tasks, with nothing in its inbox data")]
fn ticktick_reports_its_tasks(world: &mut ApiWorld) {
    world.ticktick_tasks.reported = Some(ticktick_tasks_response());
}

#[given(expr = "I already have the task {string} of the second TickTick task, in my Inbox")]
async fn i_already_have_the_task_of_the_second_item(world: &mut ApiWorld, title: String) {
    let user_id = world.user().id;
    let items = ticktick_tasks_response();
    let app = world.app.get().await;
    let creation = create_task_third_party_item(
        app,
        ThirdPartyItemData::TickTickItem(Box::new(TickTickItem {
            title,
            content: Some("more details".to_string()),
            priority: TickTickItemPriority::High,
            tags: Some(vec![TickTickTag::new("tag1")]),
            project_id: INBOX_PROJECT_ID.to_string(),
            ..items[1].clone()
        })),
        user_id,
    )
    .await;
    world.ticktick_tasks.known.push(*creation);
}

#[given("I already have the task of the Linear issue, mirrored by a TickTick item")]
async fn i_have_a_task_mirrored_by_ticktick(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let linear_id = world.integrations.linear_connection.as_ref().unwrap().id;
    let ticktick_id = world.integrations.ticktick_connection.as_ref().unwrap().id;
    let issues: Vec<LinearIssue> = sync_linear_tasks_response()
        .data
        .unwrap()
        .try_into()
        .unwrap();
    let app = world.app.get().await;
    let task = create_linear_task_with_ticktick_sink(
        app,
        &issues[0],
        ProjectSummary {
            name: "Project2".to_string(),
            source_id: PROJECT_2_ID.into(),
        },
        user_id,
        linear_id,
        ticktick_id,
        MIRRORING_ITEM_ID.to_string(),
        PROJECT_2_ID.to_string(),
    )
    .await;
    world.integrations.linear_task = Some(task);
}

#[given(expr = "the user renamed that item {string} in TickTick")]
fn the_user_renamed_the_item(world: &mut ApiWorld, title: String) {
    let task = world.integrations.linear_task.as_ref().unwrap();
    let ThirdPartyItemData::TickTickItem(sink_item) = task.sink_item.as_ref().unwrap().data.clone()
    else {
        panic!("Expected the sink item to be a TickTick item");
    };
    world.ticktick_tasks.reported = Some(vec![TickTickItem {
        title,
        ..*sink_item
    }]);
}

async fn mount_tasks(world: &mut ApiWorld) {
    let Some(items) = world.ticktick_tasks.reported.clone() else {
        return;
    };
    let app = world.app.get().await;
    let server = &app.ticktick_mock_server;
    mock_ticktick_list_projects_service(server, &ticktick_projects_response()).await;
    mock_ticktick_inbox_data_service(server, &[]).await;
    for project_id in [INBOX_PROJECT_ID, PROJECT_2_ID] {
        let of_project: Vec<TickTickItem> = items
            .iter()
            .filter(|item| item.project_id == project_id)
            .cloned()
            .collect();
        mock_ticktick_list_tasks_service(server, project_id, &of_project).await;
    }
}

#[when("I sync my TickTick tasks")]
async fn i_sync_my_ticktick_tasks(world: &mut ApiWorld) {
    mount_tasks(world).await;
    let client = world.client();
    let app = world.app.get().await;
    world.ticktick_tasks.results = sync_tasks(
        &client,
        &app.api_address,
        Some(TaskSourceKind::TickTick),
        false,
    )
    .await;
}

#[when("I ask for a sync of my TickTick tasks")]
async fn i_ask_for_a_sync(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = sync_tasks_response(
        &client,
        &app.api_address,
        Some(TaskSourceKind::TickTick),
        false,
    )
    .await;
    world.ticktick_tasks.status = Some(response.status());
}

#[then("TickTick accepts the sync request")]
fn the_sync_request_is_accepted(world: &mut ApiWorld) {
    assert_eq!(world.ticktick_tasks.status, Some(http::StatusCode::OK));
}

#[then("TickTick received no request")]
async fn ticktick_received_no_request(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let requests = app
        .ticktick_mock_server
        .received_requests()
        .await
        .unwrap()
        .len();
    assert_eq!(requests, 0);
}

#[then("every TickTick task became a task")]
fn every_task_became_a_task(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let items = world.ticktick_tasks.reported.clone().unwrap();
    assert_eq!(world.ticktick_tasks.results.len(), items.len());
    assert_sync_ticktick_items(&world.ticktick_tasks.results, &items, user_id);
}

#[then("the task I already had was updated from the second TickTick task")]
async fn the_known_task_was_updated(world: &mut ApiWorld) {
    let second = ticktick_tasks_response()[1].clone();
    let known = world.ticktick_tasks.known[0].task.clone().unwrap();
    let project = ticktick_projects_response()
        .into_iter()
        .find(|project| project.id == second.project_id)
        .expect("The second task has no project");
    let client = world.client();
    let app = world.app.get().await;
    let task: Box<Task> = get_resource(&client, &app.api_address, "tasks", known.id.into()).await;
    assert_eq!(task.id, known.id);
    assert_eq!(task.source_item.source_id, known.source_item.source_id);
    assert_eq!(task.title, second.title);
    assert_eq!(task.body, second.content.clone().unwrap_or_default());
    assert_eq!(
        task.tags,
        second
            .tags
            .iter()
            .flatten()
            .map(|tag| tag.name.to_string())
            .collect::<Vec<_>>()
    );
    assert_eq!(task.project, project.name);
    assert_eq!(
        task.source_item.data,
        ThirdPartyItemData::TickTickItem(Box::new(second))
    );
}

#[then(
    expr = "the notification of that task was deleted when it left my Inbox, and is still titled {string}"
)]
async fn its_notification_was_deleted(world: &mut ApiWorld, title: String) {
    let known = world.ticktick_tasks.known[0].clone();
    let client = world.client();
    let app = world.app.get().await;
    let notification: Box<NotificationWithTask> = get_resource(
        &client,
        &app.api_address,
        "notifications",
        known.notification.as_ref().unwrap().id.into(),
    )
    .await;
    assert_eq!(
        notification.task.as_ref().map(|task| task.id),
        known.task.as_ref().map(|task| task.id)
    );
    assert_eq!(
        notification.source_item.source_id,
        known.notification.as_ref().unwrap().source_item.source_id
    );
    assert_eq!(notification.status, NotificationStatus::Deleted);
    assert_eq!(notification.title, title);
}

#[then("the new task of the first TickTick task, which is in my Inbox, has an unread notification")]
async fn the_new_task_has_a_notification(world: &mut ApiWorld) {
    let first_id = world.ticktick_tasks.reported.as_ref().unwrap()[0]
        .id
        .clone();
    let creation = world
        .ticktick_tasks
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

#[then(
    "my TickTick connection recorded the date of the sync and a completed tasks sync, without failure"
)]
async fn the_connection_recorded_a_completed_sync(world: &mut ApiWorld) {
    let id = world.integrations.ticktick_connection.as_ref().unwrap().id;
    let connection = stored_connection(world, id).await;
    match &connection.provider {
        IntegrationProvider::TickTick { context, config } => {
            assert!(context.as_ref().unwrap().last_sync_at.is_some());
            assert_eq!(*config, TickTickConfig::enabled());
        }
        _ => panic!("Expected TickTick provider"),
    }
    assert!(connection.last_tasks_sync_started_at.is_some());
    assert!(connection.last_tasks_sync_completed_at.is_some());
    assert!(connection.last_tasks_sync_failed_at.is_none());
    assert!(connection.last_tasks_sync_failure_message.is_none());
    assert_eq!(connection.tasks_sync_failures, 0);
    assert_eq!(connection.status, IntegrationConnectionStatus::Validated);
    assert!(connection.failure_message.is_none());
}

#[then(expr = "the sync updated the task I already had, which is now titled {string}")]
async fn the_sync_adopted_the_rename(world: &mut ApiWorld, title: String) {
    let known = world.integrations.linear_task.as_ref().unwrap().id;
    let results = &world.ticktick_tasks.results;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].task.id, known);
    let client = world.client();
    let app = world.app.get().await;
    let task = get_task(&client, &app.api_address, known).await.unwrap();
    assert_eq!(task.title, title);
}
