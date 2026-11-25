use chrono::{TimeDelta, Utc};
use std::str::FromStr;

use cucumber::{Parameter, given, then, when};
use pretty_assertions::assert_eq;
use serde_json::json;

use universal_inbox::{
    integration_connection::{
        IntegrationConnectionPausedReason, IntegrationConnectionStatus,
        config::IntegrationConnectionConfig, integrations::slack::SlackConfig,
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
        slack::{
            post_signed_slack_event, post_slack_event_signed_at, post_slack_event_signed_with,
            post_unsigned_slack_event,
        },
        slack_revocation::event_callback,
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
    app.repository
        .pause_integration_connection(
            &mut transaction,
            id,
            Utc::now(),
            IntegrationConnectionPausedReason::Inactivity,
        )
        .await
        .unwrap();
    transaction.commit().await.unwrap();
}

#[derive(Clone, Copy, Debug)]
pub enum Signature {
    Valid,
    Missing,
    WrongSecret,
    Stale,
    Early,
    Recent,
}

async fn send_signed(
    world: &mut ApiWorld,
    signature: Signature,
    team: &str,
    event: serde_json::Value,
    minutes_ago: i64,
) {
    let event_time = (Utc::now() - TimeDelta::minutes(minutes_ago)).timestamp();
    let body = event_callback(team, event, event_time);
    let client = world.client();
    let app = world.app.get().await;
    let (response, expected) = match signature {
        Signature::Valid => (
            post_signed_slack_event(&client, &app.api_address, &body).await,
            200,
        ),
        Signature::Missing => (
            post_unsigned_slack_event(&client, &app.api_address, &body).await,
            401,
        ),
        Signature::WrongSecret => (
            post_slack_event_signed_with(
                &client,
                &app.api_address,
                &body,
                "not-the-signing-secret",
                Utc::now(),
            )
            .await,
            401,
        ),
        Signature::Stale => (
            post_slack_event_signed_at(
                &client,
                &app.api_address,
                &body,
                Utc::now() - TimeDelta::minutes(10),
            )
            .await,
            401,
        ),
        Signature::Early => (
            post_slack_event_signed_at(
                &client,
                &app.api_address,
                &body,
                Utc::now() + TimeDelta::minutes(10),
            )
            .await,
            401,
        ),
        Signature::Recent => (
            post_slack_event_signed_at(
                &client,
                &app.api_address,
                &body,
                Utc::now() - TimeDelta::minutes(4),
            )
            .await,
            200,
        ),
    };
    assert_eq!(response.status(), expected);
    world.slack_revocation.status = Some(response.status().as_u16());
}

async fn send(world: &mut ApiWorld, team: &str, event: serde_json::Value, minutes_ago: i64) {
    send_signed(world, Signature::Valid, team, event, minutes_ago).await;
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

#[derive(Clone, Copy, Debug)]
pub enum RevocationKind {
    TokensRevoked,
    AppUninstalled,
}

impl Parameter for RevocationKind {
    const REGEX: &'static str = "tokens_revoked|app_uninstalled";
    const NAME: &'static str = "revocation_kind";
}

impl FromStr for RevocationKind {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "tokens_revoked" => Ok(Self::TokensRevoked),
            "app_uninstalled" => Ok(Self::AppUninstalled),
            other => Err(format!("Unknown revocation event {other:?}")),
        }
    }
}

impl Parameter for Signature {
    const REGEX: &'static str = "without a signature|signed with a wrong secret|signed correctly 10 minutes ago|signed correctly 10 minutes ahead";
    const NAME: &'static str = "forgery";
}

impl FromStr for Signature {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "without a signature" => Ok(Self::Missing),
            "signed with a wrong secret" => Ok(Self::WrongSecret),
            "signed correctly 10 minutes ago" => Ok(Self::Stale),
            "signed correctly 10 minutes ahead" => Ok(Self::Early),
            other => Err(format!("Unknown forgery {other:?}")),
        }
    }
}

fn revocation(kind: RevocationKind, slack_user: &str) -> serde_json::Value {
    match kind {
        RevocationKind::TokensRevoked => tokens_revoked(slack_user),
        RevocationKind::AppUninstalled => json!({ "type": "app_uninstalled" }),
    }
}

#[when(
    expr = "an attacker sends the {revocation_kind} event that would revoke the Slack user {string} of the team {string}, {forgery}"
)]
async fn an_attacker_sends_a_revocation(
    world: &mut ApiWorld,
    kind: RevocationKind,
    slack_user: String,
    team: String,
    forgery: Signature,
) {
    send_signed(world, forgery, &team, revocation(kind, &slack_user), 0).await;
}

#[when(
    expr = "Slack sends the {revocation_kind} event that revokes the Slack user {string} of the team {string}"
)]
async fn slack_sends_a_revocation(
    world: &mut ApiWorld,
    kind: RevocationKind,
    slack_user: String,
    team: String,
) {
    send(world, &team, revocation(kind, &slack_user), 0).await;
}

#[when(
    expr = "Slack sends the {revocation_kind} event that revokes the Slack user {string} of the team {string}, signed correctly 4 minutes ago"
)]
async fn slack_sends_a_revocation_signed_earlier(
    world: &mut ApiWorld,
    kind: RevocationKind,
    slack_user: String,
    team: String,
) {
    send_signed(
        world,
        Signature::Recent,
        &team,
        revocation(kind, &slack_user),
        0,
    )
    .await;
}
