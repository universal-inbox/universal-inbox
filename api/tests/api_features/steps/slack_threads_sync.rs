use std::str::FromStr;

use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;

use universal_inbox::{
    integration_connection::{
        config::IntegrationConnectionConfig, integrations::slack::SlackConfig,
    },
    notification::{NotificationSourceKind, NotificationStatus},
};

use crate::{
    common::settings,
    helpers::{
        integration_connection::{
            create_and_mock_integration_connection, slack_context, slack_oauth_credential,
        },
        notification::{
            slack::{
                create_notification_from_slack_thread, mock_slack_auth_test,
                mock_slack_fetch_channel, mock_slack_fetch_full_thread, mock_slack_fetch_team,
                mock_slack_fetch_user, mock_slack_get_chat_permalink, slack_auth_test_response,
                slack_thread,
            },
            sync_notifications,
        },
    },
    state::SlackThreadState,
    steps::{integration_connections::stored_connection, status::UserStatus},
    world::ApiWorld,
};

const CHANNEL_ID: &str = "C05XXX";
const SLACK_USER_TOKEN: &str = "slack_test_user_access_token";

impl Parameter for SlackThreadState {
    const REGEX: &'static str = "read|unsubscribed|unchanged";
    const NAME: &'static str = "slack_state";
}

impl FromStr for SlackThreadState {
    type Err = String;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name {
            "read" => Ok(Self::Read),
            "unsubscribed" => Ok(Self::Unsubscribed),
            "unchanged" => Ok(Self::Unchanged),
            other => Err(format!("Unknown Slack thread state {other:?}")),
        }
    }
}

#[given("I am connected to Slack, syncing its notifications")]
async fn i_am_connected_to_slack(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(SlackConfig::enabled_as_notifications()),
        &settings(),
        slack_oauth_credential(),
        None,
        None,
    )
    .await;
    world.integrations.slack_connection = Some(connection);
}

#[given("I already have an unread notification of a Slack thread")]
async fn i_already_have_an_unread_notification(world: &mut ApiWorld) {
    let user_id = world.user().id;
    let connection_id = world.integrations.slack_connection.as_ref().unwrap().id;
    let mut thread = *slack_thread();
    thread.last_read = None;
    let app = world.app.get().await;
    let notification =
        create_notification_from_slack_thread(app, &thread, user_id, connection_id).await;
    assert_eq!(notification.status, NotificationStatus::Unread);
}

#[given(expr = "that thread is {slack_state} in Slack")]
fn that_thread_is_in_slack(world: &mut ApiWorld, state: SlackThreadState) {
    world.slack_threads.slack_state = Some(state);
}

#[given(expr = "Slack tells me that my team is {string}")]
async fn slack_tells_my_team(world: &mut ApiWorld, team_id: String) {
    let app = world.app.get().await;
    mock_slack_auth_test(
        &app.slack_mock_server,
        SLACK_USER_TOKEN,
        slack_auth_test_response(&team_id),
        1,
    )
    .await;
    world.slack_threads.team_id = Some(team_id);
}

async fn mount_slack(world: &mut ApiWorld) {
    let Some(state) = world.slack_threads.slack_state else {
        return;
    };
    let thread = slack_thread();
    let first_message_id = thread.messages.first().origin.ts.to_string();
    let last_message_id = thread.messages.last().origin.ts.to_string();
    let (subscribed, last_read, permalink_message_id) = match state {
        SlackThreadState::Read => (true, Some(1), last_message_id),
        SlackThreadState::Unsubscribed => (false, None, first_message_id.clone()),
        SlackThreadState::Unchanged => (true, None, first_message_id.clone()),
    };
    let app = world.app.get().await;
    let server = &app.slack_mock_server;
    mock_slack_fetch_full_thread(
        server,
        CHANNEL_ID,
        &first_message_id,
        "slack_fetch_thread_response.json",
        subscribed,
        last_read,
        SLACK_USER_TOKEN,
    )
    .await;
    mock_slack_fetch_channel(server, CHANNEL_ID, "slack_fetch_channel_response.json").await;
    mock_slack_fetch_team(server, "T05XXX", "slack_fetch_team_response.json").await;
    for user in ["U01", "U02"] {
        mock_slack_fetch_user(server, user, "slack_fetch_user_response.json").await;
    }
    mock_slack_get_chat_permalink(
        server,
        CHANNEL_ID,
        &permalink_message_id,
        "slack_get_chat_permalink_response.json",
    )
    .await;
}

#[when("I sync my Slack notifications")]
async fn i_sync_my_slack_notifications(world: &mut ApiWorld) {
    mount_slack(world).await;
    let client = world.client();
    let app = world.app.get().await;
    world.slack_threads.synced = sync_notifications(
        &client,
        &app.api_address,
        Some(NotificationSourceKind::Slack),
        false,
    )
    .await;
}

#[then(expr = "I get one Slack notification, {status}")]
fn i_get_one_slack_notification(world: &mut ApiWorld, status: UserStatus) {
    assert_eq!(world.slack_threads.synced.len(), 1);
    assert_eq!(world.slack_threads.synced[0].status, status.0);
}

#[then("the sync returned no notification")]
fn the_sync_returned_no_notification_slack(world: &mut ApiWorld) {
    assert!(world.slack_threads.synced.is_empty());
}

#[then("my Slack connection stores the team context of that team")]
async fn my_connection_stores_the_team_context(world: &mut ApiWorld) {
    let team_id = world.slack_threads.team_id.clone().unwrap();
    let id = world.integrations.slack_connection.as_ref().unwrap().id;
    let connection = stored_connection(world, id).await;
    assert_eq!(connection.provider.context(), Some(slack_context(&team_id)));
}
