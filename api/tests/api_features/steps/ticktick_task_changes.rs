use chrono::{NaiveTime, TimeZone, Utc};
use cucumber::{given, then, when};
use pretty_assertions::assert_eq;

use universal_inbox::{
    HasHtmlUrl,
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::{
            github::GithubConfig, task_time_config::TaskTimeConfig, ticktick::TickTickConfig,
        },
        provider::IntegrationProviderKind,
    },
    notification::{NotificationStatus, NotificationWithTask, service::NotificationPatch},
    task::{DueDate, Task, TaskCreation, service::TaskPatch},
    third_party::{integrations::ticktick::TickTickItem, item::ThirdPartyItemData},
};
use universal_inbox_api::integrations::ticktick::TickTickCreateTaskResponse;

use universal_inbox_api::integrations::oauth2::AccessToken;

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            OAuthCredentialFixture, create_and_mock_integration_connection,
            github_oauth_credential, ticktick_oauth_credential,
        },
        notification::create_task_from_notification,
        notification::github::{create_notification_from_github_notification, github_notification},
        rest::{get_resource, patch_resource},
        task::ticktick::{
            mock_ticktick_complete_task_service, mock_ticktick_create_task_service,
            mock_ticktick_create_task_service_with_schedule, mock_ticktick_delete_task_service,
            mock_ticktick_get_task_service, mock_ticktick_update_task_service, ticktick_item,
            ticktick_projects_response,
        },
        third_party::create_task_third_party_item,
    },
    steps::todoist_task_changes::TaskState,
    world::ApiWorld,
};

const INBOX_PROJECT_ID: &str = "tt_proj_1111";
const PROJECT2_ID: &str = "tt_proj_2222";

fn response_of(item: &TickTickItem, title: &str) -> TickTickCreateTaskResponse {
    TickTickCreateTaskResponse {
        id: item.id.clone(),
        project_id: item.project_id.clone(),
        title: title.to_string(),
        content: item.content.clone(),
        desc: item.desc.clone(),
        all_day: item.all_day,
        start_date: item.start_date,
        due_date: item.due_date,
        time_zone: item.time_zone.clone(),
        priority: item.priority,
        status: item.status,
        tags: item.tags.clone(),
    }
}

fn the_task(world: &ApiWorld) -> Task {
    world
        .ticktick_task_changes
        .creation
        .as_ref()
        .expect("The scenario has no TickTick task")
        .task
        .as_ref()
        .unwrap()
        .clone()
}

fn the_item(world: &ApiWorld) -> TickTickItem {
    let ThirdPartyItemData::TickTickItem(item) = &world
        .ticktick_task_changes
        .creation
        .as_ref()
        .expect("The scenario has no TickTick task")
        .third_party_item
        .data
    else {
        panic!("The task is not a TickTick item");
    };
    (**item).clone()
}

async fn add_task(world: &mut ApiWorld, project_id: &str, content: Option<&str>) {
    let user_id = world.user().id;
    let item = TickTickItem {
        project_id: project_id.to_string(),
        content: content.map(str::to_string).or(ticktick_item().content),
        ..*ticktick_item()
    };
    let app = world.app.get().await;
    let creation = create_task_third_party_item(
        app,
        ThirdPartyItemData::TickTickItem(Box::new(item)),
        user_id,
    )
    .await;
    world.ticktick_task_changes.creation = Some(creation);
}

#[given("I have a TickTick task in my Inbox")]
async fn i_have_a_task_in_my_inbox(world: &mut ApiWorld) {
    add_task(world, INBOX_PROJECT_ID, None).await;
}

#[given("I have a TickTick task in the project Project2, with the body \"existing body\"")]
async fn i_have_a_task_in_project2(world: &mut ApiWorld) {
    add_task(world, PROJECT2_ID, Some("existing body")).await;
}

#[given("the notification of that TickTick task is unread")]
#[then("the notification of that TickTick task is still unread")]
async fn the_notification_is_unread(world: &mut ApiWorld) {
    assert_eq!(notification_status(world).await, NotificationStatus::Unread);
}

async fn notification_status(world: &mut ApiWorld) -> NotificationStatus {
    let id = world
        .ticktick_task_changes
        .creation
        .as_ref()
        .unwrap()
        .notification
        .as_ref()
        .expect("The task has no notification")
        .id;
    let client = world.client();
    let app = world.app.get().await;
    let notification: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", id.into()).await;
    notification.status
}

#[given("TickTick deletes that task, once")]
async fn ticktick_deletes_that_task(world: &mut ApiWorld) {
    let item = the_item(world);
    let app = world.app.get().await;
    mock_ticktick_delete_task_service(&app.ticktick_mock_server, &item.project_id, &item.id).await;
}

#[given("TickTick completes that task, once")]
async fn ticktick_completes_that_task(world: &mut ApiWorld) {
    let item = the_item(world);
    let app = world.app.get().await;
    mock_ticktick_complete_task_service(&app.ticktick_mock_server, &item.project_id, &item.id)
        .await;
}

#[given(expr = "TickTick renames that task to {string}, once, with its real project id")]
async fn ticktick_renames_that_task(world: &mut ApiWorld, title: String) {
    let item = the_item(world);
    let app = world.app.get().await;
    mock_ticktick_update_task_service(
        &app.ticktick_mock_server,
        &item.id,
        &item.project_id,
        &title,
        &response_of(&item, &title),
    )
    .await;
}

#[given(
    "TickTick adds the notification title to the body of that task, once, with its real project id"
)]
async fn ticktick_appends_the_link(world: &mut ApiWorld) {
    let item = the_item(world);
    let title = world
        .integrations
        .github_notification
        .as_ref()
        .expect("The account has no GitHub notification")
        .title
        .clone();
    let app = world.app.get().await;
    mock_ticktick_update_task_service(
        &app.ticktick_mock_server,
        &item.id,
        &item.project_id,
        &title,
        &response_of(&item, &item.title),
    )
    .await;
}

async fn patch_task(world: &mut ApiWorld, patch: TaskPatch) {
    let id = the_task(world).id;
    let client = world.client();
    let app = world.app.get().await;
    let patched: Box<Task> =
        patch_resource(&client, &app.api_address, "tasks", id.into(), &patch).await;
    world.ticktick_task_changes.patched = Some(patched);
}

#[when(expr = "I mark that TickTick task as {task_state}")]
async fn i_mark_the_task(world: &mut ApiWorld, state: TaskState) {
    patch_task(
        world,
        TaskPatch {
            status: Some(state.0),
            ..Default::default()
        },
    )
    .await;
}

#[when(expr = "I rename that TickTick task to {string}")]
async fn i_rename_the_task(world: &mut ApiWorld, title: String) {
    patch_task(
        world,
        TaskPatch {
            title: Some(title),
            ..Default::default()
        },
    )
    .await;
}

#[then(expr = "the patched TickTick task is {task_state}")]
fn the_patched_task_is(world: &mut ApiWorld, state: TaskState) {
    let existing = the_task(world);
    let patched = world
        .ticktick_task_changes
        .patched
        .as_ref()
        .expect("The task was not patched");
    assert_eq!(
        **patched,
        Task {
            status: state.0,
            completed_at: patched.completed_at,
            ..existing
        }
    );
}

#[then("the patched TickTick task has a completion date")]
fn the_patched_task_has_a_completion_date(world: &mut ApiWorld) {
    assert!(
        world
            .ticktick_task_changes
            .patched
            .as_ref()
            .unwrap()
            .completed_at
            .is_some()
    );
}

#[then("the notification of that TickTick task is deleted")]
async fn the_notification_is_deleted(world: &mut ApiWorld) {
    assert_eq!(
        notification_status(world).await,
        NotificationStatus::Deleted
    );
}

#[then(
    expr = "the patched TickTick task is titled {string}, and so is that task when I read it again"
)]
async fn the_patched_task_is_titled(world: &mut ApiWorld, title: String) {
    let existing = the_task(world);
    let patched = world.ticktick_task_changes.patched.as_ref().unwrap();
    assert_eq!(
        **patched,
        Task {
            title: title.clone(),
            ..existing.clone()
        }
    );
    let client = world.client();
    let app = world.app.get().await;
    let reloaded: Box<Task> =
        get_resource(&client, &app.api_address, "tasks", existing.id.into()).await;
    assert_eq!(reloaded.title, title);
}

fn the_body_link(world: &ApiWorld) -> Option<String> {
    let notification = world.integrations.github_notification.as_ref()?;
    Some(format!(
        "- [{}]({})",
        notification.title,
        notification.get_html_url().as_ref()
    ))
}

#[given(
    "TickTick creates the item in the project Project2, as the Universal Inbox item, and then serves it"
)]
async fn ticktick_creates_the_item(world: &mut ApiWorld) {
    let item = TickTickItem {
        project_id: PROJECT2_ID.to_string(),
        ..*ticktick_item()
    };
    world.ticktick_task_changes.item_id = Some(item.id.clone());
    let app = world.app.get().await;
    mock_ticktick_create_task_service(
        &app.ticktick_mock_server,
        &item.title,
        Some(PROJECT2_ID),
        item.priority,
        &response_of(&item, &item.title),
    )
    .await;
    mock_ticktick_get_task_service(&app.ticktick_mock_server, &item.project_id, &item.id, &item)
        .await;
}

#[given(
    "TickTick creates the item in the project Project2, from 12:00 to 12:30 UTC on 2026-06-29, in the time zone Europe/Paris, and then serves it"
)]
async fn ticktick_creates_the_scheduled_item(world: &mut ApiWorld) {
    let item = TickTickItem {
        project_id: PROJECT2_ID.to_string(),
        ..*ticktick_item()
    };
    world.ticktick_task_changes.item_id = Some(item.id.clone());
    let app = world.app.get().await;
    mock_ticktick_create_task_service_with_schedule(
        &app.ticktick_mock_server,
        &item.title,
        PROJECT2_ID,
        item.priority,
        "Europe/Paris",
        "2026-06-29T12:00:00Z",
        "2026-06-29T12:30:00Z",
        &response_of(&item, &item.title),
    )
    .await;
    mock_ticktick_get_task_service(&app.ticktick_mock_server, &item.project_id, &item.id, &item)
        .await;
}

async fn create_from_the_notification(
    world: &mut ApiWorld,
    due_at: Option<DueDate>,
    time_config: Option<TaskTimeConfig>,
) {
    let notification_id = world
        .integrations
        .github_notification
        .as_ref()
        .expect("The account has no GitHub notification")
        .id;
    let item = ticktick_item();
    let creation = TaskCreation {
        title: item.title.clone(),
        body: the_body_link(world),
        project_name: Some("Project2".to_string()),
        due_at,
        priority: item.priority.into(),
        task_provider_kind: Some(IntegrationProviderKind::TickTick),
        time_config,
    };
    let client = world.client();
    let app = world.app.get().await;
    let created =
        create_task_from_notification(&client, &app.api_address, notification_id, Some(creation))
            .await;
    world.ticktick_task_changes.created = Some(created);
}

#[when("I create a TickTick task from that notification, in the project Project2")]
async fn i_create_a_task(world: &mut ApiWorld) {
    create_from_the_notification(world, ticktick_item().get_due_date(), None).await;
}

#[when(
    "I create a TickTick task from that notification, in the project Project2, due at 12:00 UTC on 2026-06-29, at 14:00 for 30 minutes in Europe/Paris"
)]
async fn i_create_a_scheduled_task(world: &mut ApiWorld) {
    create_from_the_notification(
        world,
        Some(DueDate::DateTimeWithTz(
            Utc.with_ymd_and_hms(2026, 6, 29, 12, 0, 0).unwrap(),
        )),
        Some(TaskTimeConfig {
            time: NaiveTime::from_hms_opt(14, 0, 0).unwrap(),
            duration_minutes: Some(30),
            timezone: "Europe/Paris".to_string(),
        }),
    )
    .await;
}

#[then(
    "the task that I created is the TickTick item in the project Project2, and its notification is deleted and was read just now"
)]
fn the_created_task(world: &mut ApiWorld) {
    let created = world
        .ticktick_task_changes
        .created
        .as_ref()
        .and_then(Option::as_ref)
        .expect("No task was created");
    let task = created.task.as_ref().expect("The notification has no task");
    assert_eq!(created.status, NotificationStatus::Deleted);
    assert!(Utc::now() - created.last_read_at.unwrap() < chrono::Duration::minutes(1));
    assert_eq!(task.project, "Project2");
    assert_eq!(
        Some(task.source_item.source_id.clone()),
        world.ticktick_task_changes.item_id
    );
}

#[then("the notification stays linked to the TickTick task that I created")]
async fn the_notification_is_linked_to_the_created_task(world: &mut ApiWorld) {
    let task_id = world
        .ticktick_task_changes
        .created
        .as_ref()
        .and_then(Option::as_ref)
        .and_then(|created| created.task.as_ref())
        .expect("No task was created")
        .id;
    let notification_id = world.integrations.github_notification.as_ref().unwrap().id;
    let client = world.client();
    let app = world.app.get().await;
    let notification: Box<NotificationWithTask> = get_resource(
        &client,
        &app.api_address,
        "notifications",
        notification_id.into(),
    )
    .await;
    assert_eq!(notification.status, NotificationStatus::Deleted);
    assert_eq!(
        notification.task.as_ref().map(|task| task.id),
        Some(task_id)
    );
}

#[when("I link that notification to that TickTick task, deleting it")]
async fn i_link_the_notification(world: &mut ApiWorld) {
    let notification_id = world.integrations.github_notification.as_ref().unwrap().id;
    let task_id = the_task(world).id;
    let client = world.client();
    let app = world.app.get().await;
    let _: Box<universal_inbox::notification::Notification> = patch_resource(
        &client,
        &app.api_address,
        "notifications",
        notification_id.into(),
        &NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            task_id: Some(task_id),
            ..Default::default()
        },
    )
    .await;
}

#[then("that TickTick task, read again, has the title of that notification in its body")]
async fn the_task_has_the_title_in_its_body(world: &mut ApiWorld) {
    let title = world
        .integrations
        .github_notification
        .as_ref()
        .unwrap()
        .title
        .clone();
    let id = the_task(world).id;
    let client = world.client();
    let app = world.app.get().await;
    let task: Box<Task> = get_resource(&client, &app.api_address, "tasks", id.into()).await;
    assert!(
        task.body.contains(&title),
        "expected the body to contain {title:?}, got: {}",
        task.body
    );
}

#[then(
    "the TickTick create request of that creation carried a due date only: no start date and no time zone"
)]
async fn the_create_request_had_a_due_date_only(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let requests = app.ticktick_mock_server.received_requests().await.unwrap();
    let bodies: Vec<serde_json::Value> = requests
        .iter()
        .filter(|request| request.method.as_str() == "POST" && request.url.path() == "/task")
        .map(|request| serde_json::from_slice(&request.body).unwrap())
        .collect();
    assert_eq!(bodies.len(), 1, "{bodies:?}");
    assert!(bodies[0].get("dueDate").is_some(), "{}", bodies[0]);
    assert!(bodies[0].get("startDate").is_none(), "{}", bodies[0]);
    assert!(bodies[0].get("timeZone").is_none(), "{}", bodies[0]);
    assert_eq!(bodies[0]["allDay"], false);
}

#[given("TickTick is not asked anything")]
async fn ticktick_is_not_asked_anything(world: &mut ApiWorld) {
    let app = world.app.get().await;
    wiremock::Mock::given(wiremock::matchers::any())
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(0)
        .mount(&app.ticktick_mock_server)
        .await;
}

const OTHER_USER_TOKEN: &str = "other_user_ticktick_access_token";

async fn other_user_with_ticktick(
    world: &mut ApiWorld,
    ticktick_credential: OAuthCredentialFixture,
) {
    let other_user_id = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .user
        .id;
    let app = world.app.get().await;
    let github = create_and_mock_integration_connection(
        app,
        other_user_id,
        IntegrationConnectionConfig::Github(GithubConfig::enabled()),
        &settings(),
        github_oauth_credential(),
        None,
        None,
    )
    .await;
    create_and_mock_integration_connection(
        app,
        other_user_id,
        IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
        &settings(),
        ticktick_credential,
        None,
        None,
    )
    .await;
    let notification = create_notification_from_github_notification(
        app,
        &github_notification(),
        other_user_id,
        github.id,
    )
    .await;
    world.ticktick_task_changes.others_notification = Some(*notification);
}

#[given("that other user has their own TickTick task")]
async fn that_other_user_has_a_task(world: &mut ApiWorld) {
    let other_user_id = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .user
        .id;
    let item = TickTickItem {
        project_id: PROJECT2_ID.to_string(),
        ..*ticktick_item()
    };
    let app = world.app.get().await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/project"))
        .and(wiremock::matchers::header(
            "authorization",
            format!("Bearer {OTHER_USER_TOKEN}").as_str(),
        ))
        .respond_with(
            wiremock::ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(ticktick_projects_response()),
        )
        .mount(&app.ticktick_mock_server)
        .await;
    let creation = create_task_third_party_item(
        app,
        ThirdPartyItemData::TickTickItem(Box::new(item)),
        other_user_id,
    )
    .await;
    world.ticktick_task_changes.others_task = creation.task;
}

#[when("that other user creates a TickTick task from my notification, in the project Project2")]
async fn that_other_user_creates_from_mine(world: &mut ApiWorld) {
    let notification_id = world.integrations.github_notification.as_ref().unwrap().id;
    let client = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .client
        .clone();
    let item = ticktick_item();
    let creation = TaskCreation {
        title: item.title.clone(),
        body: the_body_link(world),
        project_name: Some("Project2".to_string()),
        due_at: item.get_due_date(),
        priority: item.priority.into(),
        task_provider_kind: Some(IntegrationProviderKind::TickTick),
        time_config: None,
    };
    let app = world.app.get().await;
    let response = crate::helpers::notification::create_task_from_notification_response(
        &client,
        &app.api_address,
        notification_id,
        Some(creation),
    )
    .await;
    world
        .http
        .record(crate::world::Response::read(response).await);
}

#[when("that other user links my notification to their TickTick task, deleting it")]
async fn that_other_user_links_mine(world: &mut ApiWorld) {
    let notification_id = world.integrations.github_notification.as_ref().unwrap().id;
    let task_id = world
        .ticktick_task_changes
        .others_task
        .as_ref()
        .expect("That other user has no TickTick task")
        .id;
    let client = world
        .other_session
        .as_ref()
        .expect("There is no other user")
        .client
        .clone();
    let app = world.app.get().await;
    let response = crate::helpers::rest::patch_resource_response(
        &client,
        &app.api_address,
        "notifications",
        notification_id.into(),
        &NotificationPatch {
            status: Some(NotificationStatus::Deleted),
            task_id: Some(task_id),
            ..Default::default()
        },
    )
    .await;
    world
        .http
        .record(crate::world::Response::read(response).await);
}

#[then("my GitHub notification, read again, is exactly as it was, with no task")]
async fn my_notification_is_as_it_was(world: &mut ApiWorld) {
    let before = world.integrations.github_notification.clone().unwrap();
    let client = world.client();
    let app = world.app.get().await;
    let read: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", before.id.into()).await;
    assert!(read.task.is_none());
    assert_eq!(
        universal_inbox::notification::Notification::from(*read),
        *before
    );
}

#[then("the notification of that other user, read by that other user, is exactly as it was")]
async fn their_notification_is_as_it_was(world: &mut ApiWorld) {
    let before = world
        .ticktick_task_changes
        .others_notification
        .clone()
        .expect("That other user has no notification");
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    let read: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", before.id.into()).await;
    assert_eq!(
        universal_inbox::notification::Notification::from(*read),
        before
    );
}

#[given(
    "that other user has their own TickTick connection, with an access token of their own, and their own notification of the same GitHub thread"
)]
async fn that_other_user_has_a_ticktick_connection(world: &mut ApiWorld) {
    other_user_with_ticktick(
        world,
        OAuthCredentialFixture {
            access_token: AccessToken(OTHER_USER_TOKEN.to_string()),
            ..ticktick_oauth_credential()
        },
    )
    .await;
}
