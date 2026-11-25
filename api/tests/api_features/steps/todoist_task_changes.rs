use std::str::FromStr;

use chrono::{NaiveDate, TimeZone, Utc};
use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path, path_regex},
};

use universal_inbox::{
    HasHtmlUrl,
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::todoist::TodoistConfig,
    },
    notification::{NotificationStatus, NotificationWithTask, service::NotificationPatch},
    task::{
        DueDate, PresetDueDate, ProjectSummary, Task, TaskCreation, TaskPriority, TaskStatus,
        service::TaskPatch,
    },
    third_party::{
        integrations::todoist::{TodoistItem, TodoistItemDue, TodoistItemPriority},
        item::ThirdPartyItemData,
    },
};
use universal_inbox_api::integrations::todoist::{
    TodoistSyncCommandItemCompleteArgs, TodoistSyncCommandItemMoveArgs,
    TodoistSyncCommandItemUpdateArgs,
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            create_and_mock_integration_connection, todoist_oauth_credential,
        },
        notification::create_task_from_notification,
        rest::{get_resource, patch_resource, patch_resource_response},
        task::todoist::{
            TodoistSyncPartialCommand, mock_todoist_complete_item_service,
            mock_todoist_delete_item_service, mock_todoist_get_item_service,
            mock_todoist_id_mappings_service, mock_todoist_item_add_service,
            mock_todoist_no_sync_call, mock_todoist_sync_project_add, mock_todoist_sync_service,
            mock_todoist_sync_service_expecting_one_call,
            mock_todoist_sync_service_with_command_error, mock_todoist_uncomplete_item_service,
            todoist_item,
        },
        third_party::create_task_third_party_item,
    },
    world::{ApiWorld, Response},
};

const LEGACY_TASK_ID: &str = "9735649058";
const LEGACY_PROJECT_ID: &str = "2203306141";
const NEW_PROJECT_ID: &str = "6Jf8VQXxpwv56VQ9";

#[derive(Clone, Copy, Debug)]
pub enum Project {
    Inbox,
    Project2,
}

impl Parameter for Project {
    const REGEX: &'static str = "Inbox|Project2";
    const NAME: &'static str = "todoist_project";
}

impl FromStr for Project {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "Inbox" => Ok(Self::Inbox),
            "Project2" => Ok(Self::Project2),
            other => Err(format!("Unknown project {other:?}")),
        }
    }
}

impl Project {
    fn id(self) -> &'static str {
        match self {
            Self::Inbox => "1111",
            Self::Project2 => "2222",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct TaskState(TaskStatus);

impl Parameter for TaskState {
    const REGEX: &'static str = "active|done|deleted";
    const NAME: &'static str = "task_state";
}

impl FromStr for TaskState {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "active" => Ok(Self(TaskStatus::Active)),
            "done" => Ok(Self(TaskStatus::Done)),
            "deleted" => Ok(Self(TaskStatus::Deleted)),
            other => Err(format!("Unknown task state {other:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Priority(TodoistItemPriority);

impl Parameter for Priority {
    const REGEX: &'static str = "P1|P2|P3|P4";
    const NAME: &'static str = "priority";
}

impl FromStr for Priority {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "P1" => Ok(Self(TodoistItemPriority::P1)),
            "P2" => Ok(Self(TodoistItemPriority::P2)),
            "P3" => Ok(Self(TodoistItemPriority::P3)),
            "P4" => Ok(Self(TodoistItemPriority::P4)),
            other => Err(format!("Unknown priority {other:?}")),
        }
    }
}

fn the_task_id(world: &ApiWorld) -> universal_inbox::task::TaskId {
    world
        .todoist_task_changes
        .items
        .last()
        .expect("The scenario has no Todoist task")
        .1
        .task
        .as_ref()
        .unwrap()
        .id
}

fn the_source_id(world: &ApiWorld) -> String {
    world
        .todoist_task_changes
        .items
        .last()
        .expect("The scenario has no Todoist task")
        .1
        .third_party_item
        .source_id
        .clone()
}

async fn add_task(world: &mut ApiWorld, item: TodoistItem) {
    let user_id = world.user().id;
    let source_id = item.id.clone();
    let app = world.app.get().await;
    let creation = create_task_third_party_item(
        app,
        ThirdPartyItemData::TodoistItem(Box::new(item)),
        user_id,
    )
    .await;
    world.todoist_task_changes.items.push((source_id, creation));
}

#[given(expr = "I have a Todoist task, added in the year 2000, in the project {todoist_project}")]
async fn i_have_a_task(world: &mut ApiWorld, project: Project) {
    let item = TodoistItem {
        project_id: project.id().to_string(),
        added_at: Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap(),
        ..*todoist_item()
    };
    add_task(world, item).await;
}

#[given(expr = "I have a Todoist task with the old id {string}")]
async fn i_have_a_task_with_an_old_id(world: &mut ApiWorld, id: String) {
    let item = TodoistItem {
        id,
        project_id: LEGACY_PROJECT_ID.to_string(),
        ..*todoist_item()
    };
    add_task(world, item).await;
}

#[given(expr = "I have a Todoist task with the new id {string}")]
async fn i_have_a_task_with_a_new_id(world: &mut ApiWorld, id: String) {
    let item = TodoistItem {
        id,
        project_id: NEW_PROJECT_ID.to_string(),
        ..*todoist_item()
    };
    add_task(world, item).await;
}

#[given(
    "I am connected to Todoist, with the default project Project2, the due date today and the priority of the Todoist item"
)]
async fn connected_with_defaults(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let item = todoist_item();
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Todoist(TodoistConfig {
            default_project: Some(ProjectSummary {
                source_id: "2222".to_string().into(),
                name: "Project2".to_string(),
            }),
            default_due_at: Some(PresetDueDate::Today),
            default_priority: Some(TaskPriority::from(item.priority)),
            ..TodoistConfig::enabled()
        }),
        &settings(),
        todoist_oauth_credential(),
        None,
        None,
    )
    .await;
    world.integrations.todoist_connection = Some(connection);
}

#[given("Todoist deletes that task")]
async fn todoist_deletes(world: &mut ApiWorld) {
    let source_id = the_source_id(world);
    let app = world.app.get().await;
    mock_todoist_delete_item_service(&app.todoist_mock_server, &source_id).await;
}

#[given("Todoist completes the task that I have")]
async fn todoist_completes(world: &mut ApiWorld) {
    let source_id = the_source_id(world);
    let app = world.app.get().await;
    mock_todoist_complete_item_service(&app.todoist_mock_server, &source_id).await;
}

#[given(expr = "Todoist creates the project {string} with the id {string}")]
async fn todoist_creates_a_project(world: &mut ApiWorld, name: String, id: String) {
    let app = world.app.get().await;
    mock_todoist_sync_project_add(&app.todoist_mock_server, &name, &id).await;
    world.todoist_task_changes.new_project_id = Some(id);
}

#[given(
    expr = "Todoist moves that task to the new project, and updates it with the due date {string} and the priority {priority}"
)]
async fn todoist_moves_and_updates(world: &mut ApiWorld, due: String, priority: Priority) {
    let source_id = the_source_id(world);
    let project_id = world
        .todoist_task_changes
        .new_project_id
        .clone()
        .expect("No new project");
    let due: NaiveDate = due.parse().unwrap();
    let item_id = todoist_item().id;
    let app = world.app.get().await;
    mock_todoist_uncomplete_item_service(&app.todoist_mock_server, &item_id).await;
    mock_todoist_sync_service(
        &app.todoist_mock_server,
        vec![
            TodoistSyncPartialCommand::ItemMove {
                args: TodoistSyncCommandItemMoveArgs {
                    id: source_id.clone(),
                    project_id,
                },
            },
            TodoistSyncPartialCommand::ItemUpdate {
                args: TodoistSyncCommandItemUpdateArgs {
                    id: source_id,
                    due: Some(Some(TodoistItemDue {
                        string: "".to_string(),
                        date: DueDate::Date(due),
                        is_recurring: false,
                        timezone: None,
                        lang: "en".to_string(),
                    })),
                    priority: Some(priority.0),
                    description: None,
                    content: None,
                },
            },
        ],
        None,
    )
    .await;
}

#[given(
    expr = "Todoist renames that task to {string}, in exactly one call that changes nothing else"
)]
async fn todoist_renames(world: &mut ApiWorld, title: String) {
    let source_id = the_source_id(world);
    let app = world.app.get().await;
    mock_todoist_sync_service_expecting_one_call(
        &app.todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemUpdate {
            args: TodoistSyncCommandItemUpdateArgs {
                id: source_id,
                content: Some(title),
                ..Default::default()
            },
        }],
        None,
    )
    .await;
}

#[given("Todoist is not asked to change anything")]
async fn todoist_is_not_asked(world: &mut ApiWorld) {
    let app = world.app.get().await;
    mock_todoist_no_sync_call(&app.todoist_mock_server).await;
}

#[given("Todoist is told to add a link to that notification to the description of that task")]
async fn todoist_adds_the_link(world: &mut ApiWorld) {
    let source_id = the_source_id(world);
    let notification = world.integrations.github_notification.as_ref().unwrap();
    let description = format!(
        "\n- [{}]({})",
        notification.title,
        notification.get_html_url().as_ref()
    );
    let app = world.app.get().await;
    mock_todoist_sync_service(
        &app.todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemUpdate {
            args: TodoistSyncCommandItemUpdateArgs {
                id: source_id,
                description: Some(description),
                ..Default::default()
            },
        }],
        None,
    )
    .await;
}

#[given("GitHub accepts to mark that notification as done")]
async fn github_accepts_done(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(method("DELETE"))
        .and(path("/notifications/threads/1"))
        .respond_with(ResponseTemplate::new(205))
        .mount(&app.github_mock_server)
        .await;
}

#[given(
    expr = "Todoist adds the item in the project {todoist_project}, as {string}, and then serves it"
)]
async fn todoist_adds_the_item(world: &mut ApiWorld, project: Project, title: String) {
    let notification = world.integrations.github_notification.as_ref().unwrap();
    let body = Some(format!(
        "- [{}]({})",
        notification.title,
        notification.get_html_url().as_ref()
    ));
    let item = TodoistItem {
        project_id: project.id().to_string(),
        ..*todoist_item()
    };
    let due = item.due.as_ref().map(DueDate::from);
    let app = world.app.get().await;
    mock_todoist_item_add_service(
        &app.todoist_mock_server,
        &item.id,
        title,
        body,
        Some(item.project_id.clone()),
        due.as_ref().map(|due| due.into()),
        item.priority,
    )
    .await;
    mock_todoist_get_item_service(&app.todoist_mock_server, Box::new(item)).await;
}

#[given(
    "Todoist adds the item with the defaults of my connection, as the title of that notification, and then serves it"
)]
async fn todoist_adds_the_item_with_defaults(world: &mut ApiWorld) {
    let notification = world.integrations.github_notification.as_ref().unwrap();
    let title = notification.title.clone();
    let body = Some(format!(
        "- [{}]({})",
        notification.title,
        notification.get_html_url().as_ref()
    ));
    let item = TodoistItem {
        project_id: "2222".to_string(),
        ..*todoist_item()
    };
    let app = world.app.get().await;
    mock_todoist_item_add_service(
        &app.todoist_mock_server,
        &item.id,
        title,
        body,
        Some("2222".to_string()),
        Some((&Into::<DueDate>::into(PresetDueDate::Today)).into()),
        item.priority,
    )
    .await;
    mock_todoist_get_item_service(&app.todoist_mock_server, Box::new(item)).await;
}

#[given(expr = "Todoist rejects, with the error 557, the completion of the task {string}")]
async fn todoist_rejects_a_completion(world: &mut ApiWorld, id: String) {
    let app = world.app.get().await;
    mock_todoist_sync_service_with_command_error(
        &app.todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemComplete {
            args: TodoistSyncCommandItemCompleteArgs { id },
        }],
        557,
    )
    .await;
}

#[given(expr = "Todoist rejects, with the error 557, the rename of the task {string} to {string}")]
async fn todoist_rejects_a_rename(world: &mut ApiWorld, id: String, title: String) {
    let app = world.app.get().await;
    mock_todoist_sync_service_with_command_error(
        &app.todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemUpdate {
            args: TodoistSyncCommandItemUpdateArgs {
                id,
                content: Some(title),
                ..Default::default()
            },
        }],
        557,
    )
    .await;
}

#[given(expr = "Todoist renames the task {string} to {string}, exactly once")]
async fn todoist_renames_by_id(world: &mut ApiWorld, id: String, title: String) {
    let app = world.app.get().await;
    mock_todoist_sync_service_expecting_one_call(
        &app.todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemUpdate {
            args: TodoistSyncCommandItemUpdateArgs {
                id,
                content: Some(title),
                ..Default::default()
            },
        }],
        None,
    )
    .await;
}

#[given(expr = "Todoist completes the task {string}")]
async fn todoist_completes_by_id(world: &mut ApiWorld, id: String) {
    let app = world.app.get().await;
    mock_todoist_complete_item_service(&app.todoist_mock_server, &id).await;
}

#[given(expr = "Todoist maps the old {word} id {string} to the new id {string}")]
async fn todoist_maps_an_id(world: &mut ApiWorld, object: String, old: String, new: String) {
    let app = world.app.get().await;
    mock_todoist_id_mappings_service(
        &app.todoist_mock_server,
        &format!("{object}s"),
        vec![(&old, &new)],
    )
    .await;
}

#[given("Todoist maps no old task id")]
async fn todoist_maps_no_task(world: &mut ApiWorld) {
    let app = world.app.get().await;
    mock_todoist_id_mappings_service(&app.todoist_mock_server, "tasks", vec![]).await;
}

#[given("Todoist maps the old task ids to the new ones, and the old project id to the new one")]
async fn todoist_maps_the_ids(world: &mut ApiWorld) {
    let app = world.app.get().await;
    mock_todoist_id_mappings_service(
        &app.todoist_mock_server,
        "tasks",
        vec![
            (LEGACY_TASK_ID, "6Jf8VQXxpwv56VQ7"),
            ("9735649059", "6Jf8VQXxpwv56VQ8"),
        ],
    )
    .await;
    mock_todoist_id_mappings_service(
        &app.todoist_mock_server,
        "projects",
        vec![(LEGACY_PROJECT_ID, NEW_PROJECT_ID)],
    )
    .await;
}

#[given("Todoist refuses the id mappings, as the access token was revoked")]
async fn todoist_refuses_the_mappings(world: &mut ApiWorld) {
    let app = world.app.get().await;
    Mock::given(method("GET"))
        .and(path_regex("^/id_mappings/"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&app.todoist_mock_server)
        .await;
}

async fn patch_task(world: &mut ApiWorld, patch: TaskPatch) -> Response {
    let id = the_task_id(world);
    let client = world.client();
    let app = world.app.get().await;
    let response =
        patch_resource_response(&client, &app.api_address, "tasks", id.into(), &patch).await;
    Response::read(response).await
}

async fn patch_task_ok(world: &mut ApiWorld, patch: TaskPatch) {
    let id = the_task_id(world);
    let client = world.client();
    let app = world.app.get().await;
    let patched: Box<Task> =
        patch_resource(&client, &app.api_address, "tasks", id.into(), &patch).await;
    world.todoist_task_changes.patched = Some(patched);
}

#[when(expr = "I mark that task as {task_state}")]
async fn i_mark_the_task(world: &mut ApiWorld, state: TaskState) {
    patch_task_ok(
        world,
        TaskPatch {
            status: Some(state.0),
            ..Default::default()
        },
    )
    .await;
}

#[when(
    expr = "I move that task to the project {string}, with the due date {string} and the priority {priority}, and keep it active"
)]
async fn i_replan_the_task(world: &mut ApiWorld, project: String, due: String, priority: Priority) {
    patch_task_ok(
        world,
        TaskPatch {
            project_name: Some(project),
            due_at: Some(Some(DueDate::Date(due.parse().unwrap()))),
            priority: Some(priority.0.into()),
            status: Some(TaskStatus::Active),
            ..Default::default()
        },
    )
    .await;
}

#[when(expr = "I rename that task to {string}")]
async fn i_rename_the_task(world: &mut ApiWorld, title: String) {
    patch_task_ok(
        world,
        TaskPatch {
            title: Some(title),
            ..Default::default()
        },
    )
    .await;
}

#[when("I rename that task to the title that it already has")]
async fn i_rename_to_the_same_title(world: &mut ApiWorld) {
    let title = world
        .todoist_task_changes
        .items
        .last()
        .unwrap()
        .1
        .task
        .as_ref()
        .unwrap()
        .title
        .clone();
    let response = patch_task(
        world,
        TaskPatch {
            title: Some(title),
            ..Default::default()
        },
    )
    .await;
    world.http.record(response);
}

#[when(expr = "I mark that task as done and rename it to {string}")]
async fn i_mark_done_and_rename(world: &mut ApiWorld, title: String) {
    patch_task_ok(
        world,
        TaskPatch {
            status: Some(TaskStatus::Done),
            title: Some(title),
            ..Default::default()
        },
    )
    .await;
}

#[when("I try to mark that task as done")]
async fn i_try_to_mark_done(world: &mut ApiWorld) {
    let response = patch_task(
        world,
        TaskPatch {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .await;
    world.http.record(response);
}

#[when(
    "I create a task from that notification, with the title, the due date and the priority of the Todoist item, in the project Project2"
)]
async fn i_create_a_task_with_details(world: &mut ApiWorld) {
    let notification = world.integrations.github_notification.as_ref().unwrap();
    let body = Some(format!(
        "- [{}]({})",
        notification.title,
        notification.get_html_url().as_ref()
    ));
    let notification_id = notification.id;
    let item = todoist_item();
    let client = world.client();
    let app = world.app.get().await;
    let created = create_task_from_notification(
        &client,
        &app.api_address,
        notification_id,
        Some(TaskCreation {
            title: item.content.clone(),
            body,
            project_name: Some("Project2".to_string()),
            due_at: item.due.as_ref().map(|due| due.into()),
            priority: item.priority.into(),
            task_provider_kind: None,
            time_config: None,
        }),
    )
    .await;
    world.todoist_task_changes.created = Some(created);
}

#[when("I create a task from that notification, without any detail")]
async fn i_create_a_task_without_details(world: &mut ApiWorld) {
    let notification_id = world.integrations.github_notification.as_ref().unwrap().id;
    let client = world.client();
    let app = world.app.get().await;
    let created =
        create_task_from_notification(&client, &app.api_address, notification_id, None).await;
    world.todoist_task_changes.created = Some(created);
}

#[when("I link that notification to that task")]
async fn i_link_the_notification(world: &mut ApiWorld) {
    let notification_id = world.integrations.github_notification.as_ref().unwrap().id;
    let task_id = the_task_id(world);
    let client = world.client();
    let app = world.app.get().await;
    let patched: Box<universal_inbox::notification::Notification> = patch_resource(
        &client,
        &app.api_address,
        "notifications",
        notification_id.into(),
        &NotificationPatch {
            task_id: Some(task_id),
            ..Default::default()
        },
    )
    .await;
    assert_eq!(patched.task_id, Some(task_id));
}

async fn fetch_task(world: &mut ApiWorld, id: universal_inbox::task::TaskId) -> Box<Task> {
    let client = world.client();
    let app = world.app.get().await;
    get_resource(&client, &app.api_address, "tasks", id.into()).await
}

#[given(expr = "that Todoist task is {task_state}")]
#[then(expr = "that Todoist task is {task_state}")]
async fn that_task_is(world: &mut ApiWorld, state: TaskState) {
    let id = the_task_id(world);
    let task = fetch_task(world, id).await;
    assert_eq!(task.status, state.0);
}

#[then(expr = "the patched task is {task_state}")]
fn the_patched_task_is(world: &mut ApiWorld, state: TaskState) {
    assert_eq!(
        world.todoist_task_changes.patched.as_ref().unwrap().status,
        state.0
    );
}

#[then("the patched task has a completion date")]
fn the_patched_task_has_a_completion_date(world: &mut ApiWorld) {
    assert!(
        world
            .todoist_task_changes
            .patched
            .as_ref()
            .unwrap()
            .completed_at
            .is_some()
    );
}

#[then("the notification of that task is deleted")]
async fn the_notification_of_the_task_is_deleted(world: &mut ApiWorld) {
    let id = world
        .todoist_task_changes
        .items
        .last()
        .unwrap()
        .1
        .notification
        .as_ref()
        .unwrap()
        .id;
    let client = world.client();
    let app = world.app.get().await;
    let notification: Box<NotificationWithTask> =
        get_resource(&client, &app.api_address, "notifications", id.into()).await;
    assert_eq!(notification.status, NotificationStatus::Deleted);
}

#[then(
    expr = "the patched task is in the project {string}, due {string}, with the priority {priority}"
)]
fn the_patched_task_is_planned(
    world: &mut ApiWorld,
    project: String,
    due: String,
    priority: Priority,
) {
    let task = world.todoist_task_changes.patched.as_ref().unwrap();
    assert_eq!(task.project, project);
    assert_eq!(task.due_at, Some(DueDate::Date(due.parse().unwrap())));
    assert_eq!(task.priority, TaskPriority::from(priority.0));
}

#[given(
    expr = "that task is still in the project {string}, due {string}, with the lowest priority"
)]
#[then(expr = "that task is still in the project {string}, due {string}, with the lowest priority")]
async fn that_task_was_in(world: &mut ApiWorld, project: String, due: String) {
    let task = world
        .todoist_task_changes
        .items
        .last()
        .unwrap()
        .1
        .task
        .as_ref()
        .unwrap();
    assert_eq!(task.project, project);
    assert_eq!(task.due_at, Some(DueDate::Date(due.parse().unwrap())));
    assert_eq!(task.priority, TaskPriority::P4);
}

#[then(expr = "the patched task is titled {string}, and so is that task when I read it again")]
async fn the_patched_task_is_titled(world: &mut ApiWorld, title: String) {
    assert_eq!(
        world.todoist_task_changes.patched.as_ref().unwrap().title,
        title
    );
    let id = the_task_id(world);
    assert_eq!(fetch_task(world, id).await.title, title);
}

#[then(
    "the task that I created is the Todoist item in the project Project2, and its notification is deleted and was read just now"
)]
fn the_created_task(world: &mut ApiWorld) {
    let created = world
        .todoist_task_changes
        .created
        .as_ref()
        .and_then(Option::as_ref)
        .expect("No task was created");
    let task = created.task.as_ref().expect("The notification has no task");
    assert_eq!(created.status, NotificationStatus::Deleted);
    assert!(Utc::now() - created.last_read_at.unwrap() < chrono::Duration::minutes(1));
    assert_eq!(task.project, "Project2");
    assert_eq!(task.source_item.source_id, todoist_item().id);
}

#[then("the task that I created has the title, the due date and the priority of the Todoist item")]
fn the_created_task_has_the_details(world: &mut ApiWorld) {
    let item = todoist_item();
    let created = world
        .todoist_task_changes
        .created
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap();
    let task = created.task.as_ref().unwrap();
    assert_eq!(task.title, item.content);
    assert_eq!(task.due_at, item.due.as_ref().map(|due| due.into()));
    assert_eq!(task.priority, TaskPriority::from(item.priority));
}

#[then("the notification stays linked to the task that I created")]
async fn the_notification_is_linked_to_the_created_task(world: &mut ApiWorld) {
    let task_id = world
        .todoist_task_changes
        .created
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()
        .task
        .as_ref()
        .unwrap()
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

#[then("that notification is linked to that task, when I read it again")]
async fn the_notification_is_linked(world: &mut ApiWorld) {
    let task_id = the_task_id(world);
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
    assert_eq!(
        notification.task.as_ref().map(|task| task.id),
        Some(task_id)
    );
}

#[when("the legacy Todoist ids are migrated")]
async fn the_legacy_ids_are_migrated(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    universal_inbox_api::universal_inbox::audit::actor_context::with_actor(
        universal_inbox::audit::AuditActor::Cli {
            command: "todoist".to_string(),
        },
        universal_inbox_api::commands::todoist::migrate_legacy_ids(
            app.task_service.clone(),
            app.integration_connection_service.clone(),
            Some(user_id),
            false,
        ),
    )
    .await
    .unwrap();
}

async fn item_ids_of(
    world: &mut ApiWorld,
    task_id: universal_inbox::task::TaskId,
) -> (TaskStatus, String, String) {
    let task = fetch_task(world, task_id).await;
    let ThirdPartyItemData::TodoistItem(item) = &task.source_item.data else {
        panic!("Expected a Todoist item, got {:?}", task.source_item.data);
    };
    assert_eq!(task.source_item.source_id, item.id);
    (task.status, item.id.clone(), item.project_id.clone())
}

#[then(expr = "that task is {task_state}, with the Todoist ids {string} and {string}")]
async fn that_task_has_ids(world: &mut ApiWorld, state: TaskState, id: String, project_id: String) {
    let task_id = the_task_id(world);
    assert_eq!(item_ids_of(world, task_id).await, (state.0, id, project_id));
}

#[then(
    expr = "the task that had the id {string} is {task_state}, with the Todoist ids {string} and {string}"
)]
async fn the_task_that_had_an_id(
    world: &mut ApiWorld,
    old_id: String,
    state: TaskState,
    id: String,
    project_id: String,
) {
    let task_id = world
        .todoist_task_changes
        .items
        .iter()
        .find(|(source_id, _)| *source_id == old_id)
        .unwrap_or_else(|| panic!("No task had the id {old_id}"))
        .1
        .task
        .as_ref()
        .unwrap()
        .id;
    assert_eq!(item_ids_of(world, task_id).await, (state.0, id, project_id));
}

#[then("no legacy Todoist item is left for me")]
async fn no_legacy_item_is_left(world: &mut ApiWorld) {
    use universal_inbox_api::repository::third_party::ThirdPartyItemRepository;
    let user_id = world.user().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let legacy_items = app
        .repository
        .find_legacy_todoist_items(&mut transaction, Some(user_id))
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(legacy_items, vec![]);
}
