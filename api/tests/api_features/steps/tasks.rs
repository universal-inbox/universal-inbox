use chrono::{TimeZone, Utc};
use cucumber::{gherkin::Step, given, then, when};
use pretty_assertions::assert_eq;
use serde_json::json;
use uuid::Uuid;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::ticktick::TickTickConfig,
        provider::IntegrationProviderKind,
    },
    task::{
        ProjectSummary, TASK_BODY_MAX_LENGTH, TASK_PROJECT_NAME_MAX_LENGTH, TASK_TITLE_MAX_LENGTH,
        TaskStatus, service::TaskPatch,
    },
    third_party::{
        integrations::{ticktick::TickTickItem, todoist::TodoistItem},
        item::ThirdPartyItemData,
    },
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            create_and_mock_integration_connection, create_ticktick_integration_connection,
            todoist_oauth_credential,
        },
        rest::{get_resource_response, patch_resource_response},
        task::{
            list_tasks, search_projects, search_tasks,
            ticktick::{
                mock_ticktick_list_projects_service, ticktick_item, ticktick_projects_response,
            },
            todoist::todoist_item,
        },
        third_party::create_task_third_party_item,
    },
    world::{ApiWorld, Response},
};

fn names_in_table(step: &Step) -> Vec<String> {
    step.table
        .as_ref()
        .expect("The tasks must be given as a table")
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect()
}

fn parse_status(word: &str) -> TaskStatus {
    match word {
        "active" => TaskStatus::Active,
        "done" => TaskStatus::Done,
        other => panic!("Unknown task status {other:?}"),
    }
}

async fn create_todoist_task(
    world: &mut ApiWorld,
    name: &str,
    owner: Owner,
    customize: impl FnOnce(&mut TodoistItem),
) {
    let index = world.tasks.created;
    world.tasks.created += 1;
    let mut item = todoist_item();
    item.project_id = "1111".to_string();
    item.added_at = Utc.with_ymd_and_hms(2000, 1, 1, 0, 0, 0).unwrap();
    if index > 0 {
        item.id = format!("task-item-{index}");
    }
    customize(&mut item);
    let user_id = match owner {
        Owner::Me => world.user().id,
        Owner::TheOtherUser => world.other_session.as_ref().unwrap().user.id,
    };
    let app = world.app.get().await;
    let creation =
        create_task_third_party_item(app, ThirdPartyItemData::TodoistItem(item), user_id).await;
    world
        .tasks
        .by_name
        .insert(name.to_string(), creation.task.unwrap());
}

#[derive(Clone, Copy)]
enum Owner {
    Me,
    TheOtherUser,
}

#[given(expr = "I have a Todoist task {string}")]
async fn i_have_a_todoist_task(world: &mut ApiWorld, name: String) {
    create_todoist_task(world, &name, Owner::Me, |_| {}).await;
}

#[given(expr = "I have a Todoist task {string} titled {string}, with the description {string}")]
async fn i_have_a_titled_todoist_task(
    world: &mut ApiWorld,
    name: String,
    title: String,
    description: String,
) {
    create_todoist_task(world, &name, Owner::Me, |item| {
        item.content = title;
        item.description = description;
    })
    .await;
}

#[given(expr = "I have a done Todoist task {string}")]
async fn i_have_a_done_todoist_task(world: &mut ApiWorld, name: String) {
    create_todoist_task(world, &name, Owner::Me, |item| {
        item.checked = true;
        item.completed_at = Some(Utc.with_ymd_and_hms(2022, 1, 2, 0, 0, 0).unwrap());
    })
    .await;
    assert_eq!(world.tasks.named(&name).status, TaskStatus::Done);
}

#[given(expr = "that other user is connected to Todoist")]
async fn that_other_user_is_connected_to_todoist(world: &mut ApiWorld) {
    let user_id = world.other_session.as_ref().expect("No other user").user.id;
    let app = world.app.get().await;
    create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Todoist(
            universal_inbox::integration_connection::integrations::todoist::TodoistConfig::enabled(
            ),
        ),
        &settings(),
        todoist_oauth_credential(),
        None,
        None,
    )
    .await;
}

#[given(expr = "that other user has a Todoist task {string}")]
async fn that_other_user_has_a_todoist_task(world: &mut ApiWorld, name: String) {
    create_todoist_task(world, &name, Owner::TheOtherUser, |item| {
        item.id = "victim-item".to_string();
    })
    .await;
}

#[given("I am connected to TickTick")]
async fn i_am_connected_to_ticktick(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    create_ticktick_integration_connection(
        app,
        user_id,
        &settings(),
        IntegrationConnectionConfig::TickTick(TickTickConfig::enabled()),
        None,
    )
    .await;
    mock_ticktick_list_projects_service(&app.ticktick_mock_server, &ticktick_projects_response())
        .await;
}

#[given(expr = "I have a TickTick task {string} titled {string}")]
async fn i_have_a_ticktick_task(world: &mut ApiWorld, name: String, title: String) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let creation = create_task_third_party_item(
        app,
        ThirdPartyItemData::TickTickItem(Box::new(TickTickItem {
            title,
            project_id: "tt_proj_1111".to_string(),
            ..*ticktick_item()
        })),
        user_id,
    )
    .await;
    world.tasks.by_name.insert(name, creation.task.unwrap());
}

#[when(expr = "I get the task {string}")]
async fn i_get_the_task(world: &mut ApiWorld, name: String) {
    let id = world.tasks.named(&name).id;
    let client = world.client();
    let app = world.app.get().await;
    let response = get_resource_response(&client, &app.api_address, "tasks", id.into()).await;
    world.http.record(Response::read(response).await);
}

#[when(expr = "that other user gets the task {string}")]
async fn that_other_user_gets_the_task(world: &mut ApiWorld, name: String) {
    let id = world.tasks.named(&name).id;
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    let response = get_resource_response(&client, &app.api_address, "tasks", id.into()).await;
    world.http.record(Response::read(response).await);
}

#[when("I get a task that does not exist")]
async fn i_get_an_unknown_task(world: &mut ApiWorld) {
    let id = Uuid::new_v4();
    world.tasks.unknown_id = Some(id);
    let client = world.client();
    let app = world.app.get().await;
    let response = get_resource_response(&client, &app.api_address, "tasks", id).await;
    world.http.record(Response::read(response).await);
}

#[then(expr = "the error message is {string}, with <id> being the id of the task {string}")]
fn the_error_message_names_a_task(world: &mut ApiWorld, message: String, name: String) {
    let id = world.tasks.named(&name).id;
    assert_error_body(world, &message.replace("<id>", &id.to_string()));
}

#[then(expr = "the error message is {string}, with <id> being the id of that unknown task")]
fn the_error_message_names_the_unknown_task(world: &mut ApiWorld, message: String) {
    let id = world.tasks.unknown_id.expect("No unknown id was used");
    assert_error_body(world, &message.replace("<id>", &id.to_string()));
}

fn assert_error_body(world: &ApiWorld, message: &str) {
    assert_eq!(
        world.http.last().text(),
        json!({ "message": message }).to_string()
    );
}

#[when(regex = r"^I list my (active|done) tasks$")]
async fn i_list_my_tasks(world: &mut ApiWorld, status: String) {
    let client = world.client();
    let app = world.app.get().await;
    world.tasks.listed = list_tasks(&client, &app.api_address, parse_status(&status), false).await;
}

#[when("that other user lists their done tasks")]
async fn that_other_user_lists_their_done_tasks(world: &mut ApiWorld) {
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    world.tasks.listed = list_tasks(&client, &app.api_address, TaskStatus::Done, false).await;
}

#[then("I see no tasks")]
fn i_see_no_tasks(world: &mut ApiWorld) {
    assert!(world.tasks.listed.is_empty());
}

#[then(expr = "I see only the task {string}")]
fn i_see_only_the_task(world: &mut ApiWorld, name: String) {
    assert_eq!(world.tasks.listed, vec![world.tasks.named(&name).clone()]);
}

async fn patch(world: &mut ApiWorld, id: Uuid, patch: &impl serde::Serialize, other: bool) {
    let client = if other {
        world.other_session.as_ref().unwrap().client.clone()
    } else {
        world.client()
    };
    let app = world.app.get().await;
    let response = patch_resource_response(&client, &app.api_address, "tasks", id, patch).await;
    world.http.record(Response::read(response).await);
}

#[when(expr = "I send {string} the status it already has")]
async fn i_send_the_status_it_already_has(world: &mut ApiWorld, name: String) {
    let task = world.tasks.named(&name).clone();
    patch(
        world,
        task.id.into(),
        &TaskPatch {
            status: Some(task.status),
            ..Default::default()
        },
        false,
    )
    .await;
}

#[when(expr = "that other user marks {string} as done")]
async fn that_other_user_marks_a_task_as_done(world: &mut ApiWorld, name: String) {
    let id = world.tasks.named(&name).id;
    patch(
        world,
        id.into(),
        &TaskPatch {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
        true,
    )
    .await;
}

#[when(expr = "I point {string} at the stored item of {string}")]
async fn i_point_a_task_at_a_stored_item(world: &mut ApiWorld, name: String, target: String) {
    let id = world.tasks.named(&name).id;
    let sink_item_id = world.tasks.named(&target).source_item.id;
    patch(
        world,
        id.into(),
        &TaskPatch {
            sink_item_id: Some(sink_item_id),
            ..Default::default()
        },
        false,
    )
    .await;
}

#[then("the app does not answer 403")]
fn the_app_does_not_answer_403(world: &mut ApiWorld) {
    assert_ne!(world.http.last().status, reqwest::StatusCode::FORBIDDEN);
}

#[when(expr = "I send the task {string} a patch with nothing to change")]
async fn i_send_an_empty_patch(world: &mut ApiWorld, name: String) {
    let id = world.tasks.named(&name).id;
    patch(world, id.into(), &TaskPatch::default(), false).await;
}

#[when("I mark a task that does not exist as active")]
async fn i_patch_an_unknown_task(world: &mut ApiWorld) {
    let id = Uuid::new_v4();
    world.tasks.unknown_id = Some(id);
    patch(
        world,
        id,
        &TaskPatch {
            status: Some(TaskStatus::Active),
            ..Default::default()
        },
        false,
    )
    .await;
}

#[when(expr = "I send a patch with {word} {word} to a task that does not exist")]
async fn i_send_an_invalid_patch(world: &mut ApiWorld, field: String, problem: String) {
    let invalid = match (field.as_str(), problem.as_str()) {
        ("title", "empty") => TaskPatch {
            title: Some(String::new()),
            ..Default::default()
        },
        ("title", "too-long") => TaskPatch {
            title: Some("a".repeat(TASK_TITLE_MAX_LENGTH as usize + 1)),
            ..Default::default()
        },
        ("body", "too-long") => TaskPatch {
            body: Some("a".repeat(TASK_BODY_MAX_LENGTH as usize + 1)),
            ..Default::default()
        },
        ("project-name", "too-long") => TaskPatch {
            project_name: Some("a".repeat(TASK_PROJECT_NAME_MAX_LENGTH as usize + 1)),
            ..Default::default()
        },
        other => panic!("Unknown invalid patch {other:?}"),
    };
    patch(world, Uuid::new_v4(), &invalid, false).await;
}

#[when(expr = "I send a patch with the unknown field {string} to a task that does not exist")]
async fn i_send_a_patch_with_an_unknown_field(world: &mut ApiWorld, field: String) {
    patch(
        world,
        Uuid::new_v4(),
        &json!({ "status": "Done", field: Uuid::new_v4() }),
        false,
    )
    .await;
}

#[when(expr = "I search my tasks for {string}")]
async fn i_search_my_tasks(world: &mut ApiWorld, query: String) {
    let client = world.client();
    let app = world.app.get().await;
    world.tasks.found = search_tasks(&client, &app.api_address, &query, None).await;
}

#[when(expr = "I search my {word} tasks for {string}")]
async fn i_search_my_tasks_of_a_provider(world: &mut ApiWorld, provider: String, query: String) {
    let kind = match provider.as_str() {
        "Todoist" => IntegrationProviderKind::Todoist,
        "TickTick" => IntegrationProviderKind::TickTick,
        other => panic!("Unknown task provider {other:?}"),
    };
    let client = world.client();
    let app = world.app.get().await;
    world.tasks.found = search_tasks(&client, &app.api_address, &query, Some(kind)).await;
}

#[when(expr = "that other user searches their tasks for {string}")]
async fn that_other_user_searches_tasks(world: &mut ApiWorld, query: String) {
    let client = world.other_session.as_ref().unwrap().client.clone();
    let app = world.app.get().await;
    world.tasks.found = search_tasks(&client, &app.api_address, &query, None).await;
}

#[then("I find no task")]
fn i_find_no_task(world: &mut ApiWorld) {
    assert!(world.tasks.found.is_empty());
}

#[then(expr = "I find only the task {string}")]
fn i_find_only_the_task(world: &mut ApiWorld, name: String) {
    assert_eq!(world.tasks.found.len(), 1);
    assert_eq!(world.tasks.found[0].id, world.tasks.named(&name).id);
}

#[then("I find exactly these tasks, in any order:")]
fn i_find_exactly_these_tasks(world: &mut ApiWorld, step: &Step) {
    let mut expected: Vec<_> = names_in_table(step)
        .iter()
        .map(|name| world.tasks.named(name).id.0)
        .collect();
    expected.sort();
    let mut found: Vec<_> = world.tasks.found.iter().map(|t| t.id.0).collect();
    found.sort();
    assert_eq!(found, expected);
}

#[then(expr = "I find {int} tasks")]
fn i_find_tasks(world: &mut ApiWorld, count: usize) {
    let titles: Vec<&String> = world.tasks.found.iter().map(|t| &t.title).collect();
    assert_eq!(world.tasks.found.len(), count, "found: {titles:?}");
}

#[then(expr = "the task I find is titled {string}")]
fn the_task_i_find_is_titled(world: &mut ApiWorld, title: String) {
    assert_eq!(world.tasks.found.len(), 1);
    assert_eq!(world.tasks.found[0].title, title);
}

#[then(expr = "the title of {string} is {string}")]
fn the_title_of_a_task_is(world: &mut ApiWorld, name: String, title: String) {
    assert_eq!(world.tasks.named(&name).title, title);
}

#[when(expr = "I search my projects for {string}")]
async fn i_search_my_projects(world: &mut ApiWorld, query: String) {
    let client = world.client();
    let app = world.app.get().await;
    world.tasks.projects = search_projects(&client, &app.api_address, &query, None).await;
}

#[then(expr = "I find only the project {string} with the id {string}")]
fn i_find_only_the_project(world: &mut ApiWorld, name: String, source_id: String) {
    assert_eq!(world.tasks.projects.len(), 1);
    assert_eq!(
        world.tasks.projects[0],
        ProjectSummary {
            source_id: source_id.as_str().into(),
            name,
        }
    );
}
