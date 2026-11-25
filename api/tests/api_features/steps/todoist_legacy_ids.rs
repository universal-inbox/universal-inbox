use std::str::FromStr;

use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path_regex},
};

use universal_inbox::{
    task::TaskStatus,
    third_party::{integrations::todoist::TodoistItem, item::ThirdPartyItemData},
};
use universal_inbox_api::integrations::todoist::{
    TodoistSyncCommandItemCompleteArgs, TodoistSyncCommandItemUpdateArgs,
};

use crate::{
    helpers::task::todoist::{
        TodoistSyncPartialCommand, mock_todoist_id_mappings_service,
        mock_todoist_sync_service_expecting_one_call, mock_todoist_sync_service_with_command_error,
        todoist_item,
    },
    steps::todoist_task_changes::{TaskState, add_task, expect_once, fetch_task, the_task_id},
    world::ApiWorld,
};

const LEGACY_TASK_ID: &str = "9735649058";
const LEGACY_PROJECT_ID: &str = "2203306141";
const NEW_PROJECT_ID: &str = "6Jf8VQXxpwv56VQ9";

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

#[given(expr = "Todoist rejects, with the error 557, the completion of the task {string}")]
async fn todoist_rejects_a_completion(world: &mut ApiWorld, id: String) {
    let app = world.app.get().await;
    mock_todoist_sync_service_with_command_error(
        &app.todoist_mock_server,
        vec![TodoistSyncPartialCommand::ItemComplete {
            args: TodoistSyncCommandItemCompleteArgs { id },
        }],
        557,
        Some(1),
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
        Some(1),
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
    expect_once(
        &app.todoist_mock_server,
        TodoistSyncPartialCommand::ItemComplete {
            args: TodoistSyncCommandItemCompleteArgs { id },
        },
    )
    .await;
}

#[derive(Clone, Copy, Debug)]
pub enum Mapped {
    Task,
    Project,
}

impl Parameter for Mapped {
    const REGEX: &'static str = "task|project";
    const NAME: &'static str = "mapped_object";
}

impl FromStr for Mapped {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "task" => Ok(Self::Task),
            "project" => Ok(Self::Project),
            other => Err(format!("Unknown mapped object {other:?}")),
        }
    }
}

#[given(expr = "Todoist maps the old {mapped_object} id {string} to the new id {string}")]
async fn todoist_maps_an_id(world: &mut ApiWorld, object: Mapped, old: String, new: String) {
    let object = match object {
        Mapped::Task => "tasks",
        Mapped::Project => "projects",
    };
    let app = world.app.get().await;
    mock_todoist_id_mappings_service(&app.todoist_mock_server, object, vec![(&old, &new)]).await;
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

#[when("the legacy Todoist ids are migrated")]
async fn the_legacy_ids_are_migrated(world: &mut ApiWorld) {
    let dry_run = false;
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
            dry_run,
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
