use std::str::FromStr;

use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;
use reqwest::Client;

use universal_inbox::{
    integration_connection::provider::IntegrationProviderKind,
    notification::{NotificationStatus, NotificationWithTask},
    third_party::integrations::ticktick::{TickTickItem, TickTickItemPriority},
    user::{UserId, UserPreferences, UserPreferencesPatch},
};
use universal_inbox_api::integrations::ticktick::TickTickCreateTaskResponse;

use crate::{
    helpers::{
        rest::get_resource,
        task::ticktick::{
            mock_ticktick_create_task_service, mock_ticktick_get_task_service, ticktick_item,
        },
    },
    world::{ApiWorld, Response},
};

#[derive(Clone, Copy, Debug)]
pub struct TaskManager(IntegrationProviderKind);

impl Parameter for TaskManager {
    const REGEX: &'static str = "TickTick|Todoist";
    const NAME: &'static str = "task_manager";
}

impl FromStr for TaskManager {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "TickTick" => Ok(Self(IntegrationProviderKind::TickTick)),
            "Todoist" => Ok(Self(IntegrationProviderKind::Todoist)),
            other => Err(format!("Unknown task manager {other:?}")),
        }
    }
}

fn the_other_client(world: &ApiWorld) -> (Client, UserId) {
    let session = world
        .other_session
        .as_ref()
        .expect("There is no other user");
    (session.client.clone(), session.user.id)
}

async fn read_preferences(world: &mut ApiWorld, client: Client) {
    let app = world.app.get().await;
    let response = client
        .get(format!("{}users/me/preferences", app.api_address))
        .send()
        .await
        .expect("Failed to read the preferences");
    world.http.record(Response::read(response).await);
}

async fn set_default(world: &mut ApiWorld, client: Client, task_manager: TaskManager) {
    let app = world.app.get().await;
    let response = client
        .patch(format!("{}users/me/preferences", app.api_address))
        .json(&UserPreferencesPatch {
            default_task_manager_provider_kind: Some(Some(task_manager.0)),
            ..Default::default()
        })
        .send()
        .await
        .expect("Failed to patch the preferences");
    world.http.record(Response::read(response).await);
}

#[when("I read my preferences")]
async fn i_read_my_preferences(world: &mut ApiWorld) {
    let client = world.client();
    read_preferences(world, client).await;
}

#[given(expr = "I set my default task manager to {task_manager}")]
#[when(expr = "I set my default task manager to {task_manager}")]
async fn i_set_my_default_task_manager(world: &mut ApiWorld, task_manager: TaskManager) {
    let client = world.client();
    set_default(world, client, task_manager).await;
}

#[given(expr = "that other user set their default task manager to {task_manager}")]
#[when(expr = "that other user sets their default task manager to {task_manager}")]
async fn the_other_user_sets_their_default(world: &mut ApiWorld, task_manager: TaskManager) {
    let (client, _) = the_other_client(world);
    set_default(world, client, task_manager).await;
    assert_eq!(world.http.last().status, 200);
}

#[when("that other user reads their preferences")]
async fn the_other_user_reads_their_preferences(world: &mut ApiWorld) {
    let (client, _) = the_other_client(world);
    read_preferences(world, client).await;
}

fn the_answered_preferences(world: &ApiWorld) -> (UserId, Option<IntegrationProviderKind>) {
    let preferences: UserPreferences = world.http.last().json();
    (
        preferences.user_id,
        preferences.default_task_manager_provider_kind,
    )
}

#[then("the answer is my preferences, with no default task manager")]
fn my_preferences_have_no_default(world: &mut ApiWorld) {
    assert_eq!(the_answered_preferences(world), (world.user().id, None));
}

#[then(expr = "the answer is my preferences, with {task_manager} as my default task manager")]
fn my_preferences_have_a_default(world: &mut ApiWorld, task_manager: TaskManager) {
    assert_eq!(
        the_answered_preferences(world),
        (world.user().id, Some(task_manager.0))
    );
}

#[then("the answer is the preferences of that other user, with no default task manager")]
fn their_preferences_have_no_default(world: &mut ApiWorld) {
    let (_, user_id) = the_other_client(world);
    assert_eq!(the_answered_preferences(world), (user_id, None));
}

#[then(
    expr = "the answer is the preferences of that other user, with {task_manager} as their default task manager"
)]
fn their_preferences_have_a_default(world: &mut ApiWorld, task_manager: TaskManager) {
    let (_, user_id) = the_other_client(world);
    assert_eq!(
        the_answered_preferences(world),
        (user_id, Some(task_manager.0))
    );
}

#[given(
    "TickTick creates, without a project and without a priority, an item titled as that notification, and then serves it"
)]
async fn ticktick_creates_the_item_with_defaults(world: &mut ApiWorld) {
    let title = world
        .integrations
        .github_notification
        .as_ref()
        .expect("The account has no GitHub notification")
        .title
        .clone();
    let item = TickTickItem {
        title: title.clone(),
        priority: TickTickItemPriority::None,
        ..*ticktick_item()
    };
    let app = world.app.get().await;
    mock_ticktick_create_task_service(
        &app.ticktick_mock_server,
        &title,
        None,
        TickTickItemPriority::None,
        &TickTickCreateTaskResponse {
            id: item.id.clone(),
            project_id: item.project_id.clone(),
            title: title.clone(),
            content: item.content.clone(),
            desc: item.desc.clone(),
            all_day: item.all_day,
            start_date: item.start_date,
            due_date: item.due_date,
            time_zone: item.time_zone.clone(),
            priority: item.priority,
            status: item.status,
            tags: item.tags.clone(),
        },
    )
    .await;
    mock_ticktick_get_task_service(&app.ticktick_mock_server, &item.project_id, &item.id, &item)
        .await;
}

async fn the_notification_read_again(world: &mut ApiWorld) -> Box<NotificationWithTask> {
    let id = world
        .integrations
        .github_notification
        .as_ref()
        .expect("The account has no GitHub notification")
        .id;
    let client = world.client();
    let app = world.app.get().await;
    get_resource(&client, &app.api_address, "notifications", id.into()).await
}

#[then(
    "the task that I created without detail belongs to my TickTick connection, and that notification, read again, is deleted and linked to it"
)]
async fn the_task_belongs_to_ticktick(world: &mut ApiWorld) {
    let created = world
        .todoist_task_changes
        .created
        .clone()
        .expect("No task was created")
        .expect("The creation returned nothing");
    let task = created.task.expect("The creation returned no task");
    let ticktick_connection_id = world
        .integrations
        .ticktick_connection
        .as_ref()
        .expect("The account is not connected to TickTick")
        .id;
    assert_eq!(
        task.source_item.integration_connection_id,
        ticktick_connection_id
    );
    let notification = the_notification_read_again(world).await;
    assert_eq!(
        (
            notification.status,
            notification.task.as_ref().map(|task| task.id)
        ),
        (NotificationStatus::Deleted, Some(task.id))
    );
}

#[then("the TickTick task that I created belongs to my TickTick connection")]
fn the_explicit_task_belongs_to_ticktick(world: &mut ApiWorld) {
    let created = world
        .ticktick_task_changes
        .created
        .clone()
        .expect("No task was created")
        .expect("The creation returned nothing");
    let ticktick_connection_id = world
        .integrations
        .ticktick_connection
        .as_ref()
        .expect("The account is not connected to TickTick")
        .id;
    assert_eq!(
        created
            .task
            .expect("The creation returned no task")
            .source_item
            .integration_connection_id,
        ticktick_connection_id
    );
}

#[given(
    "I have preferences of my own, opening links in the background, without a default task manager"
)]
async fn i_have_preferences_without_a_default(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = client
        .patch(format!("{}users/me/preferences", app.api_address))
        .json(&UserPreferencesPatch {
            open_links_in_background: Some(true),
            ..Default::default()
        })
        .send()
        .await
        .expect("Failed to patch the preferences");
    world.http.record(Response::read(response).await);
    assert_eq!(the_answered_preferences(world), (world.user().id, None));
}
