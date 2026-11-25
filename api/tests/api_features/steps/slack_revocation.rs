use chrono::{TimeDelta, Utc};
use cucumber::{given, then, when};
use pretty_assertions::assert_eq;
use serde_json::json;

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionStatus, config::IntegrationConnectionConfig,
        integrations::slack::SlackConfig,
    },
    user::UserId,
};
use universal_inbox_api::repository::{
    integration_connection::{IntegrationConnectionRepository, SLACK_ACCESS_REVOKED_ERROR_MESSAGE},
    oauth_credential::OAuthCredentialRepository,
};

use crate::{
    common::settings,
    helpers::{
        auth::authenticate_user,
        integration_connection::{
            create_and_mock_integration_connection, slack_context, slack_oauth_credential,
        },
        slack::post_signed_slack_event,
        slack_revocation::{event_callback, pause_for_inactivity},
    },
    world::ApiWorld,
};

async fn connect(
    world: &mut ApiWorld,
    user_id: UserId,
    name: String,
    slack_user: &str,
    team: &str,
) {
    let mut credential = slack_oauth_credential();
    credential.provider_user_id = Some(slack_user.to_string());
    let app = world.app.get().await;
    let connection = create_and_mock_integration_connection(
        app,
        user_id,
        IntegrationConnectionConfig::Slack(SlackConfig::default()),
        &settings(),
        credential,
        None,
        Some(slack_context(team)),
    )
    .await;
    world
        .slack_revocation
        .connections
        .insert(name, connection.id);
}

#[given(
    expr = "I have the Slack connection {string} of the Slack user {string} in the team {string}"
)]
async fn i_have_a_connection(world: &mut ApiWorld, name: String, slack_user: String, team: String) {
    let user_id = world.user().id;
    connect(world, user_id, name, &slack_user, &team).await;
}

#[given(
    expr = "another user has the Slack connection {string} of the Slack user {string} in the team {string}"
)]
async fn another_user_has_a_connection(
    world: &mut ApiWorld,
    name: String,
    slack_user: String,
    team: String,
) {
    let app = world.app.get().await;
    let (_, user) = authenticate_user(
        app,
        &format!("sub-{name}"),
        "Other",
        "User",
        &format!("{name}@example.com"),
    )
    .await;
    connect(world, user.id, name, &slack_user, &team).await;
}

#[given(expr = "my Slack connection {string} is paused for inactivity, and its credential is gone")]
async fn my_connection_is_paused(world: &mut ApiWorld, name: String) {
    let id = world.slack_revocation.connections[&name];
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    app.repository
        .delete_oauth_credential(&mut transaction, id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    pause_for_inactivity(app, id).await;
}

async fn send(world: &mut ApiWorld, team: &str, event: serde_json::Value, minutes_ago: i64) {
    let event_time = (Utc::now() - TimeDelta::minutes(minutes_ago)).timestamp();
    let body = event_callback(team, event, event_time);
    let client = world.client();
    let app = world.app.get().await;
    let response = post_signed_slack_event(&client, &app.api_address, &body).await;
    assert_eq!(response.status(), 200);
    world.slack_revocation.status = Some(response.status().as_u16());
}

fn tokens_revoked(slack_users: &str) -> serde_json::Value {
    let users: Vec<&str> = slack_users
        .split(',')
        .map(str::trim)
        .filter(|user| !user.is_empty())
        .collect();
    json!({ "type": "tokens_revoked", "tokens": { "oauth": users, "bot": [] } })
}

#[when(
    expr = "Slack sends the tokens_revoked event of the team {string} for the Slack users {string}"
)]
async fn slack_sends_tokens_revoked(world: &mut ApiWorld, team: String, users: String) {
    send(world, &team, tokens_revoked(&users), 0).await;
}

#[when(
    expr = "Slack sends, {int} minutes late, the tokens_revoked event of the team {string} for the Slack users {string}"
)]
async fn slack_sends_tokens_revoked_late(
    world: &mut ApiWorld,
    minutes: i64,
    team: String,
    users: String,
) {
    send(world, &team, tokens_revoked(&users), minutes).await;
}

#[when(expr = "Slack sends the app_uninstalled event of the team {string}")]
async fn slack_sends_app_uninstalled(world: &mut ApiWorld, team: String) {
    send(world, &team, json!({ "type": "app_uninstalled" }), 0).await;
}

#[when(expr = "Slack sends, {int} minutes late, the app_uninstalled event of the team {string}")]
async fn slack_sends_app_uninstalled_late(world: &mut ApiWorld, minutes: i64, team: String) {
    send(world, &team, json!({ "type": "app_uninstalled" }), minutes).await;
}

#[then("Slack is told that the event was received")]
fn slack_is_told_it_was_received(world: &mut ApiWorld) {
    assert_eq!(world.slack_revocation.status, Some(200));
}

async fn state_of(
    world: &mut ApiWorld,
    name: &str,
) -> (IntegrationConnectionStatus, Option<String>, bool) {
    let id = world.slack_revocation.connections[name];
    let app = world.app.get().await;
    let mut transaction = app.repository.begin().await.unwrap();
    let connection = app
        .repository
        .get_integration_connection(&mut transaction, id)
        .await
        .unwrap()
        .expect("The connection should still exist");
    let credential = app
        .repository
        .get_oauth_credential(&mut transaction, id)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    (
        connection.status,
        connection.failure_message,
        credential.is_some(),
    )
}

#[then(
    expr = "the Slack connection {string} failed because its access was revoked, and its credential is gone"
)]
async fn the_connection_is_revoked(world: &mut ApiWorld, name: String) {
    assert_eq!(
        state_of(world, &name).await,
        (
            IntegrationConnectionStatus::Failing,
            Some(SLACK_ACCESS_REVOKED_ERROR_MESSAGE.to_string()),
            false
        )
    );
}

#[then(
    expr = "the Slack connection {string} is still validated, without failure, with its credential"
)]
async fn the_connection_is_untouched(world: &mut ApiWorld, name: String) {
    assert_eq!(
        state_of(world, &name).await,
        (IntegrationConnectionStatus::Validated, None, true)
    );
}

#[then(
    expr = "the Slack connection {string} is still paused, without failure, and without credential"
)]
async fn the_connection_is_still_paused(world: &mut ApiWorld, name: String) {
    assert_eq!(
        state_of(world, &name).await,
        (IntegrationConnectionStatus::Paused, None, false)
    );
}
