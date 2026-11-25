use std::str::FromStr;

use chrono::{Timelike, Utc};
use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;
use serde_json::json;
use slack_morphism::prelude::*;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{body_json, header, method, path},
};

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig,
        integrations::slack::{SlackConfig, SlackReactionConfig},
    },
    notification::{NotificationSourceKind, NotificationStatus},
    task::{Task, TaskStatus, service::TaskPatch},
    third_party::{
        integrations::{
            slack::{SlackReaction, SlackReactionItem, SlackReactionState},
            todoist::{TodoistItem, TodoistItemPriority},
        },
        item::ThirdPartyItemData,
    },
};
use universal_inbox_api::integrations::todoist::TodoistSyncCommandItemCompleteArgs;

use crate::{
    common::settings,
    helpers::{
        integration_connection::{create_and_mock_integration_connection, slack_oauth_credential},
        notification::{
            list_notifications_with_tasks,
            slack::{
                mock_slack_fetch_channel, mock_slack_fetch_reply, mock_slack_fetch_team,
                mock_slack_fetch_user, mock_slack_get_chat_permalink, mock_slack_list_emojis,
                mock_slack_list_usergroups, mock_slack_list_users_in_usergroup,
                slack_push_reaction_added_event, slack_push_reaction_removed_event,
                slack_reacted_message,
            },
        },
        rest::{get_resource, patch_resource},
        slack::post_signed_slack_event,
        task::{
            list_tasks_until,
            todoist::{
                TodoistSyncPartialCommand, mock_todoist_get_item_service,
                mock_todoist_item_add_service_expecting_one_call, sync_todoist_items_response,
                todoist_item,
            },
        },
        third_party::create_task_third_party_item,
    },
    steps::todoist_task_changes::{TaskState, expect_once},
    world::ApiWorld,
};

const CHANNEL_ID: &str = "C05XXX";
const MESSAGE_ID: &str = "1707686216.825719";
const SLACK_USER_TOKEN: &str = "Bearer slack_test_user_access_token";
const INBOX_ID: &str = "1111";
const TITLE_OF_AN_EXAMPLE_LINK: &str =
    "[📥  Universal Inbox new release 📥...](https://example.com/)";
pub(super) const TITLE_OF_A_SLACK_LINK: &str =
    "[📥  Universal Inbox new release 📥...](https://slack.com/archives/C05XXX/p1234567890)";
pub(super) const DESCRIPTION: &str = r#"📥  *Universal Inbox new release* 📥
- list 1
- list 2

1. number 1
1. number 2

> quote


```
$ echo Hello world
```
\
_Some_ `formatted` ~text~.\
\
Here is a [link](https://www.universal-inbox.com)@@john.doe@@@admins@#universal-inbox
👋![:unknown2:](https://emoji.com/unknown2.png)"#;

#[derive(Clone, Copy, Debug)]
pub enum ReactionEvent {
    Added,
    Removed,
}

impl Parameter for ReactionEvent {
    const REGEX: &'static str = "added|removed";
    const NAME: &'static str = "reaction_event";
}

impl FromStr for ReactionEvent {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "added" => Ok(Self::Added),
            "removed" => Ok(Self::Removed),
            other => Err(format!("Unknown reaction event {other:?}")),
        }
    }
}

pub(super) async fn connect_to_slack(world: &mut ApiWorld, config: SlackConfig) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(config),
        &settings(),
        slack_oauth_credential(),
        None,
        None,
    )
    .await;
    world.integrations.slack_connection = Some(connection);
}

#[given("I am connected to Slack, syncing its tasks")]
async fn i_am_connected_to_slack(world: &mut ApiWorld) {
    connect_to_slack(world, SlackConfig::enabled_as_tasks()).await;
}

#[given(
    expr = "I am connected to Slack, syncing its tasks, and marking the completed ones with the reaction {string}"
)]
async fn i_am_connected_to_slack_with_a_completion_reaction(world: &mut ApiWorld, name: String) {
    let config = SlackConfig::enabled_as_tasks();
    connect_to_slack(
        world,
        SlackConfig {
            reaction_config: SlackReactionConfig {
                completion_reaction_name: Some(SlackReactionName(name)),
                ..config.reaction_config.clone()
            },
            ..config
        },
    )
    .await;
}

#[given(
    "Slack serves the message of the reaction events, with its author, channel, team and user groups"
)]
async fn slack_serves_the_message(world: &mut ApiWorld) {
    let app = world.app.get().await;
    let server = &app.slack_mock_server;
    mock_slack_get_chat_permalink(
        server,
        CHANNEL_ID,
        MESSAGE_ID,
        "slack_get_chat_permalink_response.json",
    )
    .await;
    mock_slack_list_emojis(server, "slack_emoji_list_response.json").await;
    // The author of the message, not the user who reacted to it
    mock_slack_fetch_user(server, "U05YYY", "slack_fetch_user_response.json").await;
    mock_slack_fetch_reply(
        server,
        CHANNEL_ID,
        MESSAGE_ID,
        "slack_fetch_message_response.json",
    )
    .await;
    mock_slack_fetch_channel(server, CHANNEL_ID, "slack_fetch_channel_response.json").await;
    mock_slack_fetch_team(server, "T05XXX", "slack_fetch_team_response.json").await;
    mock_slack_list_usergroups(server, "slack_list_usergroups_response.json").await;
    mock_slack_list_users_in_usergroup(
        server,
        "S05ZZZ",
        "slack_list_users_in_usergroup_response.json",
    )
    .await;
}

async fn mount_item_add(world: &mut ApiWorld, item: TodoistItem, title: &str) {
    let app = world.app.get().await;
    mock_todoist_item_add_service_expecting_one_call(
        &app.todoist_mock_server,
        &item.id,
        title.to_string(),
        Some(DESCRIPTION.to_string()),
        Some(item.project_id.clone()),
        None,
        TodoistItemPriority::P1,
    )
    .await;
    mock_todoist_get_item_service(&app.todoist_mock_server, Box::new(item)).await;
}

#[given("Todoist adds the Slack message as an item, and then serves it")]
async fn todoist_adds_the_item(world: &mut ApiWorld) {
    mount_item_add(world, *todoist_item(), TITLE_OF_AN_EXAMPLE_LINK).await;
}

#[given(
    expr = "Todoist adds the Slack message as an item of the Inbox, and then serves it as {task_state}"
)]
async fn todoist_adds_the_item_in_the_inbox(world: &mut ApiWorld, state: TaskState) {
    let mut item = sync_todoist_items_response().items.unwrap().remove(1);
    item.project_id = INBOX_ID.to_string();
    set_state(&mut item, state);
    mount_item_add(world, item, TITLE_OF_A_SLACK_LINK).await;
}

fn set_state(item: &mut TodoistItem, state: TaskState) {
    if state.0 == TaskStatus::Done {
        item.checked = true;
        item.completed_at = Some(Utc::now());
    } else {
        item.checked = false;
        item.completed_at = None;
    }
}

#[given(expr = "Todoist then reports that item as {task_state}, in the project {string}")]
fn todoist_then_reports_the_item(world: &mut ApiWorld, state: TaskState, project_id: String) {
    let mut response = sync_todoist_items_response();
    let item = &mut response.items.as_mut().unwrap()[1];
    set_state(item, state);
    item.project_id = project_id;
    world.todoist_tasks.reported = Some(response);
}

#[when(expr = "Slack tells that the reaction {string} was {reaction_event} on that message")]
async fn slack_tells_the_reaction(world: &mut ApiWorld, _name: String, event: ReactionEvent) {
    let client = world.client();
    let app = world.app.get().await;
    let (pushed, waited_status) = match event {
        ReactionEvent::Added => (slack_push_reaction_added_event(), TaskStatus::Active),
        ReactionEvent::Removed => (slack_push_reaction_removed_event(), TaskStatus::Done),
    };
    let response = post_signed_slack_event(&client, &app.api_address, &*pushed).await;
    assert_eq!(response.status(), 200);
    let tasks = list_tasks_until(&client, &app.api_address, waited_status, 1).await;
    assert_eq!(tasks.len(), 1);
    world.slack_tasks.task = tasks.into_iter().next();
}

async fn slack_reaction(
    world: &mut ApiWorld,
    edited: Option<&str>,
) -> Box<universal_inbox::third_party::item::ThirdPartyItemCreationResult> {
    let user_id = world.user().id;
    let mut item = *slack_reacted_message();
    if let Some(title) = edited {
        let SlackReactionItem::SlackMessage(message) = &mut item else {
            panic!("Expected the reacted item to be a Slack message");
        };
        message.message.content.attachments = Some(vec![SlackMessageAttachment {
            id: None,
            color: None,
            fallback: None,
            title: Some(title.to_string()),
            fields: None,
            mrkdwn_in: None,
            text: None,
            blocks: None,
        }]);
    }
    let app = world.app.get().await;
    create_task_third_party_item(
        app,
        ThirdPartyItemData::SlackReaction(Box::new(SlackReaction {
            name: SlackReactionName("eyes".to_string()),
            state: SlackReactionState::ReactionAdded,
            created_at: Utc::now().with_nanosecond(0).unwrap(),
            item,
        })),
        user_id,
    )
    .await
}

#[given("I reacted to a Slack message, which Todoist mirrors as a task")]
async fn i_reacted_to_a_message(world: &mut ApiWorld) {
    let creation = slack_reaction(world, None).await;
    assert!(creation.task.as_ref().unwrap().sink_item.is_some());
    world.slack_tasks.creation = Some(creation);
}

#[when(expr = "that Slack message is edited upstream, with an attachment titled {string}")]
async fn the_message_is_edited(world: &mut ApiWorld, title: String) {
    let edited = slack_reaction(world, Some(&title)).await;
    world.slack_tasks.edited = Some(edited);
}

fn the_creation(
    world: &ApiWorld,
) -> &universal_inbox::third_party::item::ThirdPartyItemCreationResult {
    world
        .slack_tasks
        .creation
        .as_ref()
        .expect("The scenario has no Slack task")
}

#[then(expr = "the mirrored task is the same task, still titled as before, without {string}")]
fn the_task_keeps_its_title(world: &mut ApiWorld, edit: String) {
    let before = the_creation(world).task.as_ref().unwrap();
    let after = world
        .slack_tasks
        .edited
        .as_ref()
        .and_then(|edited| edited.task.as_ref())
        .expect("The message was not edited");
    assert_eq!(after.id, before.id);
    assert_eq!(after.title, before.title);
    assert!(
        !after.title.contains(&edit),
        "the edited Slack summary reached the task: {}",
        after.title
    );
}

#[given("Todoist completes the item that mirrors that Slack task")]
async fn todoist_completes_the_item(world: &mut ApiWorld) {
    let id = the_creation(world)
        .task
        .as_ref()
        .unwrap()
        .sink_item
        .as_ref()
        .unwrap()
        .source_id
        .clone();
    let app = world.app.get().await;
    expect_once(
        &app.todoist_mock_server,
        TodoistSyncPartialCommand::ItemComplete {
            args: TodoistSyncCommandItemCompleteArgs { id },
        },
    )
    .await;
}

fn reaction_body(name: &str) -> serde_json::Value {
    json!({ "name": name, "channel": CHANNEL_ID, "timestamp": MESSAGE_ID })
}

async fn mount_reaction_call(
    world: &mut ApiWorld,
    endpoint: &str,
    name: &str,
    answer: serde_json::Value,
) {
    let app = world.app.get().await;
    Mock::given(method("POST"))
        .and(path(endpoint))
        .and(header("authorization", SLACK_USER_TOKEN))
        .and(body_json(reaction_body(name)))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(answer),
        )
        .expect(1)
        .mount(&app.slack_mock_server)
        .await;
}

#[given(expr = "Slack removes the reaction {string} from that message, once")]
async fn slack_removes_the_reaction(world: &mut ApiWorld, name: String) {
    mount_reaction_call(world, "/reactions.remove", &name, json!({ "ok": true })).await;
}

#[given(
    expr = "Slack answers, once, that the reaction {string} is already removed from that message"
)]
async fn slack_says_the_reaction_is_gone(world: &mut ApiWorld, name: String) {
    mount_reaction_call(
        world,
        "/reactions.remove",
        &name,
        json!({ "ok": false, "error": "no_reaction" }),
    )
    .await;
}

#[given(expr = "Slack adds the reaction {string} to that message, once")]
async fn slack_adds_the_reaction(world: &mut ApiWorld, name: String) {
    mount_reaction_call(world, "/reactions.add", &name, json!({ "ok": true })).await;
}

#[when("I mark that Slack task as done")]
async fn i_mark_the_task_as_done(world: &mut ApiWorld) {
    let id = the_creation(world).task.as_ref().unwrap().id;
    let client = world.client();
    let app = world.app.get().await;
    let patched: Box<Task> = patch_resource(
        &client,
        &app.api_address,
        "tasks",
        id.into(),
        &TaskPatch {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )
    .await;
    world.slack_tasks.patched = Some(patched);
}

#[then("that task is done, with a completion date, and nothing else changed")]
fn the_task_is_done_and_nothing_else_changed(world: &mut ApiWorld) {
    let existing = the_creation(world).task.as_ref().unwrap().clone();
    let patched = world
        .slack_tasks
        .patched
        .as_ref()
        .expect("The task was not patched");
    assert!(patched.completed_at.is_some());
    assert_eq!(
        **patched,
        Task {
            status: TaskStatus::Done,
            completed_at: patched.completed_at,
            ..existing
        }
    );
}

#[then(expr = "that Slack task is {task_state}")]
fn that_slack_task_is_known(world: &mut ApiWorld, state: TaskState) {
    let task = world
        .slack_tasks
        .task
        .as_ref()
        .expect("The Slack reaction made no task");
    assert_eq!(task.status, state.0);
}

#[then(expr = "that Slack task, read again, is {task_state}")]
async fn that_slack_task_read_again_is(world: &mut ApiWorld, state: TaskState) {
    let id = world.slack_tasks.task.as_ref().unwrap().id;
    let client = world.client();
    let app = world.app.get().await;
    let task: Box<Task> = get_resource(&client, &app.api_address, "tasks", id.into()).await;
    assert_eq!(task.id, id);
    assert_eq!(task.status, state.0);
}

#[then("the sync brought two tasks, each with one notification")]
fn the_sync_brought_two_tasks_with_a_notification(world: &mut ApiWorld) {
    assert_eq!(world.todoist_tasks.results.len(), 2);
    for creation in &world.todoist_tasks.results {
        assert_eq!(creation.notifications.len(), 1);
    }
}

#[then("the sync brought two tasks")]
fn the_sync_brought_two_tasks(world: &mut ApiWorld) {
    assert_eq!(world.todoist_tasks.results.len(), 2);
}

#[then(expr = "the notification of that Slack task is {notification_status}")]
async fn the_notification_of_that_slack_task_is(
    world: &mut ApiWorld,
    status: NotificationStatusName,
) {
    let id = world.slack_tasks.task.as_ref().unwrap().id;
    let client = world.client();
    let app = world.app.get().await;
    let notifications = list_notifications_with_tasks(
        &client,
        &app.api_address,
        vec![],
        false,
        Some(id),
        Some(NotificationSourceKind::Todoist),
        false,
    )
    .await;
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].status, status.0);
}

#[then("that Slack task has no notification")]
async fn that_slack_task_has_no_notification(world: &mut ApiWorld) {
    let id = world.slack_tasks.task.as_ref().unwrap().id;
    let client = world.client();
    let app = world.app.get().await;
    let notifications = list_notifications_with_tasks(
        &client,
        &app.api_address,
        vec![],
        false,
        Some(id),
        None,
        false,
    )
    .await;
    assert_eq!(notifications.len(), 0);
}

#[derive(Clone, Copy, Debug)]
pub struct NotificationStatusName(NotificationStatus);

impl Parameter for NotificationStatusName {
    const REGEX: &'static str = "unread|deleted";
    const NAME: &'static str = "notification_status";
}

impl FromStr for NotificationStatusName {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "unread" => Ok(Self(NotificationStatus::Unread)),
            "deleted" => Ok(Self(NotificationStatus::Deleted)),
            other => Err(format!("Unknown notification status {other:?}")),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ReactionCall {
    Adds,
    Removes,
}

impl Parameter for ReactionCall {
    const REGEX: &'static str = "adds|removes";
    const NAME: &'static str = "reaction_call";
}

impl FromStr for ReactionCall {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "adds" => Ok(Self::Adds),
            "removes" => Ok(Self::Removes),
            other => Err(format!("Unknown reaction call {other:?}")),
        }
    }
}

#[given(expr = "Slack {reaction_call} the reaction {string} on that message, once")]
async fn slack_follows_the_task(world: &mut ApiWorld, call: ReactionCall, name: String) {
    let endpoint = match call {
        ReactionCall::Adds => "/reactions.add",
        ReactionCall::Removes => "/reactions.remove",
    };
    mount_reaction_call(world, endpoint, &name, json!({ "ok": true })).await;
}

#[given(
    expr = "Todoist adds the Slack message, edited upstream with an attachment titled {string}, as an item, and then serves it"
)]
async fn todoist_adds_the_edited_message(world: &mut ApiWorld, title: String) {
    mount_item_add(
        world,
        *todoist_item(),
        &format!("[{title}](https://example.com/)"),
    )
    .await;
}

#[given(
    expr = "I reacted to a Slack message that was already edited upstream, with an attachment titled {string}, which Todoist mirrors as a task"
)]
async fn i_reacted_to_an_edited_message(world: &mut ApiWorld, title: String) {
    let creation = slack_reaction(world, Some(&title)).await;
    world.slack_tasks.creation = Some(creation);
}

#[then(expr = "the mirrored task is titled after the attachment {string}")]
fn the_task_is_titled_after_the_attachment(world: &mut ApiWorld, title: String) {
    let task = the_creation(world).task.as_ref().unwrap();
    assert!(
        task.title.contains(&title),
        "the task is titled {:?}, not after the attachment",
        task.title
    );
}
