use std::str::FromStr;

use cucumber::{Parameter, given, then, when};
use pretty_assertions::{assert_eq, assert_ne};
use tokio::time::{Duration, sleep};
use wiremock::{Mock, ResponseTemplate, matchers::method, matchers::path};

use universal_inbox::{
    HasHtmlUrl,
    integration_connection::{
        IntegrationConnectionStatus,
        config::IntegrationConnectionConfig,
        integrations::linear::{LinearConfig, LinearSyncTaskConfig},
        provider::IntegrationProviderKind,
    },
    task::{
        DueDate, PresetDueDate, ProjectSummary, Task, TaskCreationConfig, TaskPriority,
        TaskSourceKind, TaskStatus, service::TaskPatch,
    },
    third_party::{
        integrations::{
            linear::LinearIssue,
            ticktick::{TickTickItem, TickTickItemPriority},
            todoist::{TodoistItem, TodoistItemPriority},
        },
        item::{ThirdPartyItem, ThirdPartyItemData, ThirdPartyItemKind},
    },
    user::UserPreferencesPatch,
};
use universal_inbox_api::{
    integrations::{
        task::ThirdPartyTaskService,
        ticktick::TickTickCreateTaskResponse,
        todoist::{TodoistSyncCommandItemCompleteArgs, TodoistSyncCommandItemUpdateArgs},
    },
    repository::{task::TaskRepository, third_party::ThirdPartyItemRepository},
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{create_and_mock_integration_connection, linear_oauth_credential},
        notification::linear::{mock_linear_assigned_issues_query, sync_linear_tasks_response},
        task::{
            get_task,
            linear::create_linear_task,
            sync_tasks,
            ticktick::{mock_ticktick_get_task_service, ticktick_item},
            todoist::{
                TodoistSyncPartialCommand, mock_todoist_complete_item_service,
                mock_todoist_get_item_service, mock_todoist_item_add_service,
                mock_todoist_no_sync_call, mock_todoist_sync_service_with_command_error,
                todoist_item,
            },
        },
    },
    state::Reported,
    steps::integration_connections::stored_connection,
    world::ApiWorld,
};

const SINK_FAILURE_NO_SUCH_ITEM: i32 = 22;
const KNOWN_TODOIST_ITEM_ID: &str = "todoist_source_id";
pub const TICKTICK_PROJECT_2_ID: &str = "tt_proj_2222";

#[derive(Clone, Debug)]
pub struct Due(pub Option<PresetDueDate>);

impl Parameter for Due {
    const REGEX: &'static str = "today|tomorrow|no date";
    const NAME: &'static str = "due";
}

impl FromStr for Due {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "today" => Ok(Self(Some(PresetDueDate::Today))),
            "tomorrow" => Ok(Self(Some(PresetDueDate::Tomorrow))),
            "no date" => Ok(Self(None)),
            other => Err(format!("Unknown due date {other:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Project {
    pub name: &'static str,
    pub source_id: &'static str,
}

impl Parameter for Project {
    const REGEX: &'static str = "Project1|Project2|TickTick Project2";
    const NAME: &'static str = "project";
}

impl FromStr for Project {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "Project1" => Ok(Self {
                name: "Project1",
                source_id: "1111",
            }),
            "Project2" => Ok(Self {
                name: "Project2",
                source_id: "2222",
            }),
            "TickTick Project2" => Ok(Self {
                name: "Project2",
                source_id: "tt_proj_2222",
            }),
            other => Err(format!("Unknown project {other:?}")),
        }
    }
}

impl Project {
    fn summary(self) -> ProjectSummary {
        ProjectSummary {
            name: self.name.to_string(),
            source_id: self.source_id.into(),
        }
    }
}

fn the_issue() -> LinearIssue {
    let issues: Vec<LinearIssue> = sync_linear_tasks_response()
        .data
        .unwrap()
        .try_into()
        .unwrap();
    issues.into_iter().next().unwrap()
}

fn title_with_link(issue: &LinearIssue) -> String {
    format!("[{}]({})", issue.title, issue.get_html_url())
}

async fn connect_linear(
    world: &mut ApiWorld,
    project: Project,
    due: Due,
    task_manager: Option<IntegrationProviderKind>,
) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Linear(LinearConfig {
            sync_notifications_enabled: true,
            sync_task_config: LinearSyncTaskConfig {
                enabled: true,
                target_project: Some(project.summary()),
                default_due_at: due.0.clone(),
                task_manager_provider_kind: task_manager,
                ..Default::default()
            },
        }),
        &settings(),
        linear_oauth_credential(),
        None,
        None,
    )
    .await;
    world.integrations.linear_connection = Some(connection);
    world.integrations.linear_project = Some(project.summary());
    world.linear_tasks.due = due.0.clone();
}

#[given(expr = "I am connected to Linear, syncing its tasks into the project {project}, due {due}")]
async fn connected_to_linear(world: &mut ApiWorld, project: Project, due: Due) {
    connect_linear(world, project, due, None).await;
}

#[given(
    expr = "I am connected to Linear, syncing its tasks into the project {project}, due {due}, to TickTick"
)]
async fn connected_to_linear_for_ticktick(world: &mut ApiWorld, project: Project, due: Due) {
    connect_linear(world, project, due, Some(IntegrationProviderKind::TickTick)).await;
}

#[given("TickTick is my default task manager")]
async fn ticktick_is_my_default_task_manager(world: &mut ApiWorld) {
    let client = world.client();
    let app = world.app.get().await;
    let response = client
        .patch(format!("{}users/me/preferences", app.api_address))
        .json(&UserPreferencesPatch {
            default_task_manager_provider_kind: Some(Some(IntegrationProviderKind::TickTick)),
            ..Default::default()
        })
        .send()
        .await
        .expect("Failed to patch user preferences");
    assert_eq!(response.status(), 200);
}

#[given("Linear reports my assigned issue")]
fn linear_reports_the_issue(world: &mut ApiWorld) {
    world.linear_tasks.reported = Reported::TheIssue;
}

#[given("Linear reports no assigned issue")]
fn linear_reports_no_issue(world: &mut ApiWorld) {
    world.linear_tasks.reported = Reported::NoIssue;
}

#[given(expr = "Linear reports that issue retitled {string}")]
fn linear_reports_a_new_title(world: &mut ApiWorld, title: String) {
    world.linear_tasks.title = Some(title);
}

#[given(expr = "Linear reports that issue with the description {string}")]
fn linear_reports_a_new_description(world: &mut ApiWorld, description: String) {
    world.linear_tasks.description = Some(description);
}

#[given(
    expr = "I already have the task of that issue in the project {project}, mirrored by Todoist"
)]
async fn i_have_a_mirrored_task(world: &mut ApiWorld, project: Project) {
    let user_id = world.user().id;
    let linear_id = world.integrations.linear_connection.as_ref().unwrap().id;
    let todoist_id = world.integrations.todoist_connection.as_ref().unwrap().id;
    let app = world.app.get().await;
    let task = create_linear_task(
        app,
        &the_issue(),
        project.summary(),
        user_id,
        linear_id,
        todoist_id,
        KNOWN_TODOIST_ITEM_ID.to_string(),
    )
    .await;
    world.linear_tasks.task = Some(task);
}

#[given(
    expr = "I already have the task of that issue in the project {project}, due today, not mirrored by any task manager"
)]
async fn i_have_an_unmirrored_task(world: &mut ApiWorld, project: Project) {
    let user_id = world.user().id;
    let linear_id = world.integrations.linear_connection.as_ref().unwrap().id;
    let issue = the_issue();
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let source_item = app
        .repository
        .create_or_update_third_party_item(
            &mut transaction,
            Box::new(ThirdPartyItem::new(
                issue.id.to_string(),
                ThirdPartyItemData::LinearIssue(Box::new(issue.clone())),
                user_id,
                linear_id,
            )),
        )
        .await
        .unwrap()
        .value();
    let request = app
        .task_service
        .read()
        .await
        .linear_service
        .third_party_item_into_task(
            &mut transaction,
            &issue,
            &source_item,
            Some(TaskCreationConfig {
                project_name: Some(project.name.to_string()),
                due_at: Some(PresetDueDate::Today.into()),
                priority: TaskPriority::default(),
                task_manager_provider_kind: None,
                time_config: None,
            }),
            user_id,
        )
        .await
        .unwrap();
    let task = app
        .repository
        .create_or_update_task(&mut transaction, request)
        .await
        .unwrap()
        .value();
    assert!(task.sink_item.is_none(), "Task should have no sink_item");
    transaction.commit().await.unwrap();
    world.linear_tasks.task = Some(*task);
}

#[given(expr = "I renamed that task {string} in Todoist")]
async fn i_renamed_the_task(world: &mut ApiWorld, title: String) {
    let user_id = world.user().id;
    let task_id = world.linear_tasks.task.as_ref().unwrap().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .update_task(
            &mut transaction,
            task_id,
            &TaskPatch {
                title: Some(title.clone()),
                ..Default::default()
            },
            user_id,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    world.linear_tasks.task.as_mut().unwrap().title = title;
}

#[given("a second passes, so that what follows is dated after what exists")]
#[given("a second passes")]
async fn a_second_passes(_world: &mut ApiWorld) {
    sleep(Duration::from_secs(1)).await;
}

#[given("Todoist accepts no call from the sync")]
async fn todoist_accepts_no_call(world: &mut ApiWorld) {
    let app = world.app.get().await;
    mock_todoist_no_sync_call(&app.todoist_mock_server).await;
}

#[given(
    expr = "Todoist creates the item {string} for the new task, titled with a link to the issue"
)]
async fn todoist_creates_the_item_for_the_new_task(world: &mut ApiWorld, item_id: String) {
    let issue = the_issue();
    let project_id = linked_project_id(world);
    let due: DueDate = world
        .linear_tasks
        .due
        .clone()
        .expect("Linear syncs no due date")
        .into();
    let app = world.app.get().await;
    mock_todoist_item_add_service(
        &app.todoist_mock_server,
        &item_id,
        title_with_link(&issue),
        issue.description.clone(),
        Some(project_id),
        Some((&due).into()),
        TodoistItemPriority::P1,
    )
    .await;
    mock_todoist_get_item_service(
        &app.todoist_mock_server,
        Box::new(TodoistItem {
            id: item_id,
            ..*todoist_item()
        }),
    )
    .await;
}

#[given(
    expr = "Todoist creates the item {string} for that task, with its title and the description Linear reports"
)]
async fn todoist_creates_the_item_for_that_task(world: &mut ApiWorld, item_id: String) {
    let task = world.linear_tasks.task.clone().unwrap();
    let description = world
        .linear_tasks
        .description
        .clone()
        .unwrap_or_else(|| task.body.clone());
    let project_id = linked_project_id(world);
    let due = task.due_at.clone().expect("The task has no due date");
    let app = world.app.get().await;
    mock_todoist_item_add_service(
        &app.todoist_mock_server,
        &item_id,
        task.title.clone(),
        Some(description),
        Some(project_id),
        Some((&due).into()),
        TodoistItemPriority::P1,
    )
    .await;
    mock_todoist_get_item_service(
        &app.todoist_mock_server,
        Box::new(TodoistItem {
            id: item_id,
            ..*todoist_item()
        }),
    )
    .await;
}

#[given("Todoist completes the item of that task")]
async fn todoist_completes_the_item(world: &mut ApiWorld) {
    let item_id = world
        .linear_tasks
        .task
        .as_ref()
        .unwrap()
        .sink_item
        .as_ref()
        .unwrap()
        .source_id
        .clone();
    let app = world.app.get().await;
    mock_todoist_complete_item_service(&app.todoist_mock_server, &item_id).await;
}

#[given("Todoist no longer knows the item of that task, so completing it fails")]
async fn todoist_no_longer_knows_the_item_on_complete(world: &mut ApiWorld) {
    let item_id = world
        .linear_tasks
        .task
        .as_ref()
        .unwrap()
        .sink_item
        .as_ref()
        .unwrap()
        .source_id
        .clone();
    let app = world.app.get().await;
    mock_todoist_sync_service_with_command_error(
        &app.todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemComplete {
            args: TodoistSyncCommandItemCompleteArgs { id: item_id },
        }],
        SINK_FAILURE_NO_SUCH_ITEM,
        None,
    )
    .await;
}

#[given("Todoist no longer knows the item of that task, so updating its description fails")]
async fn todoist_no_longer_knows_the_item_on_update(world: &mut ApiWorld) {
    let item_id = world
        .linear_tasks
        .task
        .as_ref()
        .unwrap()
        .sink_item
        .as_ref()
        .unwrap()
        .source_id
        .clone();
    let description = world.linear_tasks.description.clone().unwrap();
    let app = world.app.get().await;
    mock_todoist_sync_service_with_command_error(
        &app.todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemUpdate {
            args: TodoistSyncCommandItemUpdateArgs {
                id: item_id,
                due: None,
                priority: None,
                description: Some(description),
                content: None,
            },
        }],
        SINK_FAILURE_NO_SUCH_ITEM,
        None,
    )
    .await;
}

#[given("TickTick creates the sink item of the new task")]
async fn ticktick_creates_the_item(world: &mut ApiWorld) {
    let title = title_with_link(&the_issue());
    let item = TickTickItem {
        id: "tt_sink_from_linear".to_string(),
        project_id: "tt_proj_2222".to_string(),
        title: title.clone(),
        priority: TickTickItemPriority::High,
        ..*ticktick_item()
    };
    let app = world.app.get().await;
    Mock::given(method("POST"))
        .and(path("/task"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(TickTickCreateTaskResponse {
                    id: item.id.clone(),
                    project_id: item.project_id.clone(),
                    title,
                    content: None,
                    desc: None,
                    all_day: None,
                    start_date: None,
                    due_date: None,
                    time_zone: None,
                    priority: item.priority,
                    status: item.status,
                    tags: None,
                }),
        )
        .mount(&app.ticktick_mock_server)
        .await;
    mock_ticktick_get_task_service(&app.ticktick_mock_server, &item.project_id, &item.id, &item)
        .await;
}

#[given("I deleted that task")]
async fn i_deleted_that_task(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let task_id = world.linear_tasks.task.as_ref().unwrap().id;
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .update_task(
            &mut transaction,
            task_id,
            &TaskPatch {
                status: Some(TaskStatus::Deleted),
                ..Default::default()
            },
            user_id,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

#[when("I sync my Linear tasks")]
async fn i_sync_my_linear_tasks(world: &mut ApiWorld) {
    let tasks = &world.linear_tasks;
    let mut response = sync_linear_tasks_response();
    let data = response.data.as_mut().unwrap();
    if tasks.reported == Reported::NoIssue {
        data.issues.nodes.clear();
    } else {
        let node = &mut data.issues.nodes[0];
        if let Some(title) = &tasks.title {
            node.title = title.clone();
        }
        if let Some(description) = &tasks.description {
            node.description = Some(description.clone());
        }
    }
    let client = world.client();
    let app = world.app.get().await;
    mock_linear_assigned_issues_query(&app.linear_mock_server, &response).await;
    world.linear_tasks.results = sync_tasks(
        &client,
        &app.api_address,
        Some(TaskSourceKind::Linear),
        false,
    )
    .await;
}

fn synced_task(world: &ApiWorld) -> &Task {
    assert_eq!(world.linear_tasks.results.len(), 1);
    &world.linear_tasks.results[0].task
}

#[then("the sync returned one task, without any notification")]
fn the_sync_returned_one_task(world: &mut ApiWorld) {
    synced_task(world);
    assert!(world.linear_tasks.results[0].notifications.is_empty());
}

#[then(
    "that task comes from the Linear issue, titled with a link to it and carrying its description"
)]
fn the_task_comes_from_the_issue(world: &mut ApiWorld) {
    let issue = the_issue();
    let task = synced_task(world);
    assert_eq!(task.title, title_with_link(&issue));
    assert_eq!(task.body, issue.description.clone().unwrap_or_default());
    assert_eq!(task.kind, TaskSourceKind::Linear);
    assert_eq!(task.source_item.kind(), ThirdPartyItemKind::LinearIssue);
    assert_eq!(task.source_item.source_id, issue.id.to_string());
}

#[then(expr = "that task is active, in the project {project}, due {due}")]
fn the_task_is_active_in(world: &mut ApiWorld, project: Project, due: Due) {
    let task = synced_task(world);
    assert_eq!(task.status, TaskStatus::Active);
    assert_eq!(task.project, project.name.to_string());
    assert_eq!(task.due_at, due.0.map(Into::into));
}

#[then(expr = "that task is mirrored by the Todoist item {string}")]
fn the_task_is_mirrored_by_todoist(world: &mut ApiWorld, item_id: String) {
    let sink_item = synced_task(world).sink_item.clone().unwrap();
    assert_eq!(sink_item.kind(), ThirdPartyItemKind::TodoistItem);
    assert_eq!(sink_item.source_id, item_id);
}

#[then("that task is mirrored by a TickTick item")]
fn the_task_is_mirrored_by_ticktick(world: &mut ApiWorld) {
    let sink_item = synced_task(world)
        .sink_item
        .clone()
        .expect("expected a TickTick sink");
    assert_eq!(sink_item.kind(), ThirdPartyItemKind::TickTickItem);
    assert!(matches!(
        sink_item.data,
        ThirdPartyItemData::TickTickItem(_)
    ));
}

#[then("my Linear connection recorded a completed tasks sync, without failure")]
async fn my_connection_recorded_a_completed_sync(world: &mut ApiWorld) {
    let id = world.integrations.linear_connection.as_ref().unwrap().id;
    let connection = stored_connection(world, id).await;
    assert!(connection.last_tasks_sync_started_at.is_some());
    assert!(connection.last_tasks_sync_completed_at.is_some());
    assert!(connection.last_tasks_sync_failed_at.is_none());
    assert!(connection.last_tasks_sync_failure_message.is_none());
    assert_eq!(connection.tasks_sync_failures, 0);
    assert_eq!(connection.status, IntegrationConnectionStatus::Validated);
    assert!(connection.failure_message.is_none());
}

#[then("the sync updated the task I already had")]
fn the_sync_updated_the_task_i_had(world: &mut ApiWorld) {
    let known = world.linear_tasks.task.as_ref().unwrap().id;
    assert_eq!(synced_task(world).id, known);
}

#[then("that task is done")]
fn the_task_is_done(world: &mut ApiWorld) {
    assert_eq!(synced_task(world).status, TaskStatus::Done);
}

#[then("that task kept its project and its due date")]
async fn the_task_kept_its_project_and_due_date(world: &mut ApiWorld) {
    let known = world.linear_tasks.task.clone().unwrap();
    let client = world.client();
    let app = world.app.get().await;
    let task = get_task(&client, &app.api_address, known.id).await.unwrap();
    assert_eq!(task.due_at, known.due_at);
    assert_eq!(task.project, known.project);
}

#[then("that task kept its title")]
async fn the_task_kept_its_title(world: &mut ApiWorld) {
    let retitle = world
        .linear_tasks
        .title
        .clone()
        .expect("The scenario does not retitle the issue");
    let known = world.linear_tasks.task.clone().unwrap();
    let client = world.client();
    let app = world.app.get().await;
    let task = get_task(&client, &app.api_address, known.id).await.unwrap();
    assert_eq!(task.title, known.title);
    assert!(
        !task.title.contains(&retitle),
        "the upstream Linear title must not reach the task, got: {}",
        task.title
    );
}

#[then("Todoist received no sync request")]
async fn todoist_received_no_sync_request(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let syncs = app
        .todoist_mock_server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path() == "/sync")
        .count();
    assert_eq!(syncs, 0);
}

#[then(expr = "that task is mirrored by a new Todoist item {string}, not the one it had")]
fn the_task_is_mirrored_by_a_new_item(world: &mut ApiWorld, item_id: String) {
    let known_sink = world
        .linear_tasks
        .task
        .as_ref()
        .unwrap()
        .sink_item
        .as_ref()
        .map(|item| item.id);
    let sink_item = synced_task(world)
        .sink_item
        .clone()
        .expect("Task should now have a sink_item");
    assert_ne!(Some(sink_item.id), known_sink);
    assert_eq!(sink_item.source_id, item_id);
}

#[then(expr = "that task is titled {string}")]
async fn the_task_is_titled(world: &mut ApiWorld, title: String) {
    assert_eq!(synced_task(world).title, title);
    let id = synced_task(world).id;
    let client = world.client();
    let app = world.app.get().await;
    let task = get_task(&client, &app.api_address, id).await.unwrap();
    assert_eq!(task.title, title);
}

fn linked_project_id(world: &ApiWorld) -> String {
    world
        .integrations
        .linear_project
        .as_ref()
        .expect("The account is not connected to Linear")
        .source_id
        .to_string()
}
